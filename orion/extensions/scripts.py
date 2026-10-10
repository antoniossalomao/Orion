"""Scripts Python revisados: snapshot, argv sem shell, ambiente mínimo e prazo.

Sem sandbox de SO: código não confiável nunca chega ao runner. POSIX comprovado;
Windows permanece indisponível até validação real do controle de processos.
"""

from __future__ import annotations

import asyncio
import contextlib
import hashlib
import os
import signal
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

from ..policy import redact
from .skills import Skill, SkillError, safe_file

MAX_FILES = 256
MAX_BUNDLE = 4_000_000
MAX_OUTPUT = 32000


def snapshot(skill: Skill) -> tuple[str, dict[str, bytes]]:
    files: dict[str, bytes] = {}
    total = 0
    paths = []
    for path in skill.root.rglob("*"):
        if len(paths) >= MAX_FILES * 2:
            raise SkillError("script_bundle_too_many_entries")
        paths.append(path)
    for path in sorted(paths):
        if path.is_symlink():
            raise SkillError("symlink_forbidden")
        if path.is_dir():
            continue
        relative = path.relative_to(skill.root).as_posix()
        safe_file(skill.root, relative, limit=1_000_000)
        with path.open("rb") as stream:
            data = stream.read(1_000_001)
        total += len(data)
        if len(data) > 1_000_000 or total > MAX_BUNDLE or len(files) >= MAX_FILES:
            raise SkillError("script_bundle_too_large")
        files[relative] = data
    digest = hashlib.sha256()
    for relative, data in files.items():
        digest.update(relative.encode() + b"\0" + hashlib.sha256(data).digest())
    return digest.hexdigest(), files


def _stage(files: dict[str, bytes]) -> TemporaryDirectory:
    directory = TemporaryDirectory(prefix="orion-reviewed-script-")
    try:
        for relative, data in files.items():
            path = Path(directory.name) / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    except BaseException:
        directory.cleanup()
        raise
    return directory


class ScriptRunner:
    def __init__(self):
        self.running: set[asyncio.Task] = set()
        self.closed = False

    async def close(self) -> None:
        self.closed = True
        tasks = [t for t in self.running if t is not asyncio.current_task()]
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)

    async def run(
        self,
        skill: Skill,
        relative: str,
        argv: list[str],
        *,
        reviewed_digest: str,
        timeout_s: float = 5,
    ) -> dict:
        if self.closed:
            raise SkillError("scripts_disabled")
        if os.name != "posix":
            raise SkillError("scripts_platform_unverified")
        if (
            not relative.startswith("scripts/")
            or not relative.endswith(".py")
            or len(argv) > 32
            or any(len(arg) > 4096 or "\0" in arg for arg in argv)
        ):
            raise SkillError("script_arguments_invalid")
        body = await asyncio.to_thread(skill.load)
        if relative not in body.references:
            raise SkillError("script_not_declared")
        digest, files = await asyncio.to_thread(snapshot, skill)
        if digest != reviewed_digest:
            raise SkillError("script_revision_changed")
        if relative not in files:
            raise SkillError("script_not_found")
        # Executar os bytes revisados, não o arquivo mutável original após um await.
        directory = await asyncio.to_thread(_stage, files)
        process = None
        task = asyncio.current_task()
        if task is not None:
            self.running.add(task)
        output = bytearray()
        error_output = bytearray()
        size = 0
        readers: list[asyncio.Task] = []
        try:
            process = await asyncio.create_subprocess_exec(
                sys.executable,
                "-I",
                str(Path(directory.name) / relative),
                *argv,
                cwd=directory.name,
                env={"PATH": os.defpath, "PYTHONUNBUFFERED": "1"},
                stdin=asyncio.subprocess.DEVNULL,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                start_new_session=True,
            )

            async def read(stream, target):
                nonlocal size
                while chunk := await stream.read(4096):
                    size += len(chunk)
                    if size > MAX_OUTPUT:
                        raise SkillError("script_output_limit")
                    target.extend(chunk)

            async with asyncio.timeout(max(0.1, min(timeout_s, 30))):
                readers = [
                    asyncio.create_task(read(process.stdout, output)),
                    asyncio.create_task(read(process.stderr, error_output)),
                    asyncio.create_task(process.wait()),
                ]
                await asyncio.gather(*readers)
            return {
                "ok": process.returncode == 0,
                "exit_code": process.returncode,
                "stdout": redact(output.decode(errors="replace"), limite=MAX_OUTPUT),
                "stderr": redact(error_output.decode(errors="replace"), limite=MAX_OUTPUT),
                "revision": digest,
            }
        except TimeoutError:
            return {"ok": False, "codigo": "script_timeout"}
        finally:
            if process is not None:
                # Encerrar o grupo também quando o script deixar filhos/pipe aberto.
                with contextlib.suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
                await process.wait()
            for reader in readers:
                if not reader.done():
                    reader.cancel()
            await asyncio.gather(*readers, return_exceptions=True)
            await asyncio.to_thread(directory.cleanup)
            if task is not None:
                self.running.discard(task)
