"""Instalação por snapshot/staging; nenhuma importação, hook ou dependência é executada."""

from __future__ import annotations

import hashlib
import os
import re
import shutil
import stat
import threading
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory

from .plugins import PluginError, PluginManifest, PluginStore, parse_manifest, relative_path
from .skills import SkillError, metadata, safe_file

MAX_FILES = 512
MAX_EXPANDED = 16_000_000
MAX_FILE = 2_000_000
_RESERVED = re.compile(r"^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)", re.I)
_FORBIDDEN_DIRS = {".git", ".venv", "node_modules", "__pycache__"}


def package_path(value: str) -> str:
    relative_path(value)
    if len(value) > 1024 or "\0" in value:
        raise PluginError("bundle_path_invalid")
    if any(
        _RESERVED.match(part) or part.endswith((".", " ")) or part.casefold() in _FORBIDDEN_DIRS
        for part in value.split("/")
    ):
        raise PluginError("bundle_path_invalid")
    return value


def fingerprint(files: dict[str, bytes]) -> str:
    digest = hashlib.sha256()
    for relative, content in sorted(files.items()):
        digest.update(relative.encode() + b"\0" + hashlib.sha256(content).digest())
    return digest.hexdigest()


@dataclass(frozen=True)
class Package:
    manifest: PluginManifest
    digest: str
    files: dict[str, bytes]


def folder_snapshot(source: Path) -> Package:
    if source.is_symlink() or not source.is_dir():
        raise PluginError("bundle_source_invalid")
    files, names = {}, set()
    total, entries = 0, 0
    for path in source.rglob("*"):
        entries += 1
        if entries > MAX_FILES * 2:
            raise PluginError("bundle_too_many_entries")
        if path.is_symlink():
            raise PluginError("bundle_symlink_forbidden")
        relative = package_path(path.relative_to(source).as_posix())
        canonical = unicodedata.normalize("NFC", relative).casefold()
        if canonical in names:
            raise PluginError("bundle_name_collision")
        names.add(canonical)
        if path.is_dir():
            continue
        info = path.stat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise PluginError("bundle_special_file_forbidden")
        try:
            safe_file(source, relative, limit=MAX_FILE)
        except SkillError as error:
            raise PluginError("bundle_file_forbidden") from error
        with path.open("rb") as stream:
            content = stream.read(MAX_FILE + 1)
        total += len(content)
        if len(content) > MAX_FILE or total > MAX_EXPANDED or len(files) >= MAX_FILES:
            raise PluginError("bundle_size_limit")
        files[relative] = content
    if "manifest.json" not in files:
        raise PluginError("manifest_missing")
    return Package(parse_manifest(files["manifest.json"]), fingerprint(files), files)


def validate_package(root: Path, manifest: PluginManifest) -> None:
    try:
        ids = set()
        for relative in manifest.skills:
            package_path(relative)
            skill = metadata(root / relative, namespace=manifest.id, origin="plugin")
            skill.load()
            if skill.id in ids:
                raise PluginError("bundle_skill_collision")
            ids.add(skill.id)
        for connection in manifest.mcp:
            if connection.entrypoint:
                safe_file(root, package_path(connection.entrypoint), limit=MAX_FILE)
    except (SkillError, OSError) as error:
        raise PluginError("bundle_reference_invalid") from error


def remove_tree(root: Path) -> None:
    """Só para staging/objects do próprio installer; nunca seguir symlink no cleanup."""
    if not root.exists() and not root.is_symlink():
        return
    if root.is_symlink():
        root.unlink()
        return
    root.chmod(0o700)
    for path in root.rglob("*"):
        if path.is_symlink():
            path.unlink()
        else:
            path.chmod(0o700 if path.is_dir() else 0o600)
    shutil.rmtree(root)


class Installer:
    def __init__(self, root: Path, store: PluginStore):
        self.root = root
        self.store = store
        self.staging = root / "staging"
        self.bundles = root / "bundles"
        self.staging.mkdir(parents=True, exist_ok=True)
        self.bundles.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()

    def install_folder(self, source: Path, *, origin: str = "local", update: bool = False) -> dict:
        package = folder_snapshot(source)
        return self.install_snapshot(package, origin=origin, update=update)

    def install_snapshot(self, package: Package, *, origin: str, update: bool = False) -> dict:
        if fingerprint(package.files) != package.digest:
            raise PluginError("bundle_hash_invalid")
        with self._lock:
            # Confinamento revalidado mesmo para snapshots vindos de outros transports.
            total = 0
            for relative, data in package.files.items():
                package_path(relative)
                total += len(data)
                if len(data) > MAX_FILE or total > MAX_EXPANDED:
                    raise PluginError("bundle_size_limit")
            if len(package.files) > MAX_FILES:
                raise PluginError("bundle_size_limit")
            target = self.bundles / package.manifest.id / package.digest
            if target.exists():
                if folder_snapshot(target).digest != package.digest:
                    raise PluginError("bundle_tampered")
                self.store.record(
                    package.manifest,
                    package.digest,
                    origin,
                    bundle=target.relative_to(self.root).as_posix(),
                    update=update,
                )
                return self.store.get(package.manifest.id)
            temporary = TemporaryDirectory(prefix="install-", dir=self.staging)
            staged = Path(temporary.name)
            moved = False
            try:
                for relative, data in package.files.items():
                    path = staged / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                # Validação do snapshot completo, sem importar código Python.
                actual = folder_snapshot(staged)
                if actual.digest != package.digest or actual.manifest != package.manifest:
                    raise PluginError("bundle_hash_invalid")
                validate_package(staged, actual.manifest)
                target.parent.mkdir(parents=True, exist_ok=True)
                for path in staged.rglob("*"):
                    path.chmod(0o555 if path.is_dir() else 0o444)
                os.replace(staged, target)
                moved = True
                target.chmod(0o555)
                self.store.record(
                    package.manifest,
                    package.digest,
                    origin,
                    bundle=target.relative_to(self.root).as_posix(),
                    update=update,
                )
            except BaseException:
                if moved:
                    remove_tree(target)
                raise
            finally:
                if staged.exists():
                    remove_tree(staged)
                temporary.cleanup()
            return self.store.get(package.manifest.id)

    def bundle(self, id_: str, digest: str) -> Path:
        version = self.store.version(id_, digest)
        expected = f"bundles/{id_}/{digest}"
        if version["bundle"] != expected:
            raise PluginError("bundle_path_invalid")
        path = self.root / expected
        try:
            path.resolve().relative_to(self.root.resolve())
        except ValueError as error:
            raise PluginError("bundle_path_invalid") from error
        package = folder_snapshot(path)
        if package.digest != digest:
            raise PluginError("bundle_tampered")
        return path

    def plan_update(self, id_: str, digest: str) -> dict:
        current = self.store.get(id_)
        target = self.store.version(id_, digest)
        old = set(current["manifest"]["capabilities"])
        new = set(target["manifest"]["capabilities"])
        self.bundle(id_, digest)
        return {
            "id": id_,
            "current_digest": current["selected_digest"],
            "target_digest": digest,
            "version": target["version"],
            "added": sorted(new - old),
            "removed": sorted(old - new),
            "review_required": digest != current["selected_digest"],
        }

    def change_version(
        self, id_: str, digest: str, *, reviewed_digest: str, reviewed_capabilities: set[str]
    ) -> dict:
        with self._lock:
            self.bundle(id_, digest)
            return self.store.change_version(
                id_,
                digest,
                reviewed_digest=reviewed_digest,
                reviewed_capabilities=reviewed_capabilities,
            )

    def rollback(self, id_: str, *, reviewed_digest: str, reviewed_capabilities: set[str]) -> dict:
        previous = self.store.get(id_)["previous_digest"]
        if not previous:
            raise PluginError("plugin_rollback_unavailable")
        return self.change_version(
            id_,
            previous,
            reviewed_digest=reviewed_digest,
            reviewed_capabilities=reviewed_capabilities,
        )

    def uninstall(self, id_: str) -> None:
        with self._lock:
            versions = self.store.versions(id_)
            # Validar também ancestrais antes da operação destrutiva.
            for version in versions:
                expected = f"bundles/{id_}/{version['digest']}"
                path = self.root / expected
                if version["bundle"] != expected:
                    raise PluginError("bundle_path_invalid")
                if any(
                    parent.is_symlink()
                    for parent in [path, *path.parents]
                    if parent != self.root.parent
                ):
                    raise PluginError("bundle_path_invalid")
                try:
                    path.resolve().relative_to(self.root.resolve())
                except ValueError as error:
                    raise PluginError("bundle_path_invalid") from error
            self.store.remove(id_)
            for version in versions:
                expected = f"bundles/{id_}/{version['digest']}"
                if version["bundle"] == expected:
                    remove_tree(self.root / expected)
