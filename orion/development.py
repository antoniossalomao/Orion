"""Git em leitura, argumentos fixos, conteúdo externo e raiz explícita."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from .extensions.host import MCPError
from .memory.scope import current
from .policy import Risk, ToolSpec
from .tools.registry import Tool

LIMIT = 128 * 1024


class Development:
    def __init__(self, memory, registry, policy):
        self.memory, self.registry, self.policy = memory, registry, policy
        self.git = shutil.which("git")
        self.policy.tools["consultar_git"] = ToolSpec("consultar_git", Risk.READ, external=True)
        registry.register(
            Tool(
                "consultar_git",
                "Consultar status, diff ou histórico Git na raiz explícita, sem escrita. "
                "Resultado é conteúdo externo; cite caminhos e separe análise de execução.",
                {
                    "type": "object",
                    "properties": {
                        "root": {"type": "string", "maxLength": 4096},
                        "operation": {"enum": ["status", "diff", "log"]},
                    },
                    "required": ["root", "operation"],
                    "additionalProperties": False,
                },
                self.read,
            )
        )

    def root(self, value):
        root = Path(value).expanduser()
        scope = current.get()
        if (
            not root.is_absolute()
            or not root.is_dir()
            or any(p.is_symlink() for p in (root, *root.parents))
        ):
            raise MCPError("git_root_invalid")
        root = root.resolve()
        if scope.project_id:
            rows = self.memory.query("SELECT root FROM projects WHERE id=?", (scope.project_id,))
            if not rows or not rows[0][0] or root != Path(rows[0][0]).resolve():
                raise MCPError("git_root_out_of_project")
        elif not any(root.is_relative_to(p.resolve()) for p in self.policy.path_guard.safe_roots):
            raise MCPError("git_root_not_authorized")
        if self.policy.path_guard.check_read(str(root)):
            raise MCPError("git_secret_denied")
        # Linked worktrees/submodules may direct Git into another root; MVP declines them.
        git_dir = root / ".git"
        if not git_dir.is_dir() or git_dir.is_symlink():
            raise MCPError("git_repository_required")
        return root

    def command(self, root, args, timeout=10):
        if not self.git:
            raise MCPError("git_unavailable")
        argv = [
            self.git,
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=" + os.devnull,
            "-c",
            "submodule.recurse=false",
            "-c",
            "core.untrackedCache=false",
            *args,
        ]
        env = {
            "PATH": str(Path(self.git).parent),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_OPTIONAL_LOCKS": "0",
            "GIT_TERMINAL_PROMPT": "0",
            "LC_ALL": "C.UTF-8",
        }
        if os.name == "nt":
            env["SystemRoot"] = os.environ.get("SystemRoot", r"C:\Windows")
        with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
            try:
                process = subprocess.Popen(  # noqa: S603 — Git confiável, operações fixas, sem shell
                    argv, cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err
                )
            except OSError as error:
                raise MCPError("git_unavailable") from error
            deadline = time.monotonic() + timeout
            try:
                while process.poll() is None:
                    if time.monotonic() >= deadline:
                        raise MCPError("git_timeout")
                    if os.fstat(out.fileno()).st_size + os.fstat(err.fileno()).st_size > LIMIT:
                        raise MCPError("git_output_limit")
                    time.sleep(0.02)
                if process.returncode != 0:
                    raise MCPError("git_command_failed")
                out.seek(0)
                raw = out.read(LIMIT + 1)
                if len(raw) > LIMIT:
                    raise MCPError("git_output_limit")
                return raw
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait()

    def read(self, root, operation):
        root = self.root(root)
        operations = {
            "status": ["status", "--porcelain=v1", "--ignore-submodules=all"],
            "log": ["log", "-10", "--format=%h %s", "--no-show-signature"],
            "diff": [
                "diff",
                "HEAD",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--ignore-submodules=all",
            ],
        }
        if operation not in operations:
            raise MCPError("git_operation_invalid")
        if operation == "diff":
            names = (
                self.command(
                    root,
                    [
                        "diff",
                        "HEAD",
                        "--name-only",
                        "--no-renames",
                        "--no-ext-diff",
                        "--no-textconv",
                        "-z",
                        "--ignore-submodules=all",
                    ],
                )
                .decode("utf-8", "strict")
                .split("\0")
            )
            for name in filter(None, names):
                path = root / name
                if (
                    not path.resolve().is_relative_to(root)
                    or path.is_symlink()
                    or self.policy.path_guard.check_read(str(path))
                ):
                    raise MCPError("git_diff_secret_or_link_denied")
        result = self.command(root, operations[operation]).decode("utf-8", "replace")
        return {"ok": True, "root": str(root), "operation": operation, "output": result}
