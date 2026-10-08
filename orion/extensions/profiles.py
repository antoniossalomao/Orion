"""Pacotes mantidos no repositório, instalados somente por ação administrativa."""

from pathlib import Path

from .installer import folder_snapshot
from .plugins import PluginError

BUNDLED = {
    "orion-" + name: Path(__file__).with_name("bundled") / name
    for name in ("pesquisa", "agenda", "memoria-vault", "arquivos", "desenvolvimento")
}


def profile(id_: str):
    if id_ not in BUNDLED:
        raise PluginError("plugin_profile_not_found")
    return folder_snapshot(BUNDLED[id_])


def profiles() -> list[dict]:
    rows = []
    for id_ in BUNDLED:
        package = profile(id_)
        manifest = package.manifest
        rows.append(
            {
                "id": manifest.id,
                "name": manifest.name,
                "version": manifest.version,
                "description": manifest.description,
                "license": manifest.license,
                "capabilities": manifest.capabilities,
                "digest": package.digest,
            }
        )
    return rows
