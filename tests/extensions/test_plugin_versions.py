import json
import sqlite3

import pytest

from orion.extensions.installer import Installer
from orion.extensions.plugins import PluginError, PluginState, PluginStore


def test_prepare_update_review_atomic_pointer_rollback_and_remove(bundle, tmp_path):
    store = PluginStore(tmp_path / "data/registry.db")
    installer = Installer(tmp_path / "data/extensions", store)
    # Dados produzidos/chats ficam fora do armazenamento de bundles.
    output = tmp_path / "data/user-output.txt"
    output.write_text("resultado importante")
    try:
        first = installer.install_folder(bundle)
        v1 = first["selected_digest"]
        manifest_path = bundle / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest.update(version="2.0.0", capabilities=["native:buscar_memoria"])
        manifest_path.write_text(json.dumps(manifest))
        installer.install_folder(bundle, update=True)
        assert store.get("pesquisa")["selected_digest"] == v1
        v2 = store.versions("pesquisa")[0]["digest"]
        plan = installer.plan_update("pesquisa", v2)
        assert plan["added"] == ["native:buscar_memoria"] and plan["review_required"]
        with pytest.raises(PluginError, match="plugin_review_required"):
            installer.change_version(
                "pesquisa", v2, reviewed_digest=v1, reviewed_capabilities=set()
            )
        assert store.get("pesquisa")["selected_digest"] == v1
        # Falha real de SQLite antes do commit não publica ponteiro parcial.
        store._db.execute("""CREATE TEMP TRIGGER interromper BEFORE UPDATE ON plugins
            BEGIN SELECT RAISE(ABORT, 'fixture_interrupted'); END""")
        with pytest.raises(sqlite3.IntegrityError):
            installer.change_version(
                "pesquisa", v2, reviewed_digest=v2, reviewed_capabilities={"native:buscar_memoria"}
            )
        assert store.get("pesquisa")["selected_digest"] == v1
        store._db.execute("DROP TRIGGER interromper")
        changed = installer.change_version(
            "pesquisa", v2, reviewed_digest=v2, reviewed_capabilities={"native:buscar_memoria"}
        )
        assert changed["previous_digest"] == v1 and changed["state"] == "disabled"
        assert (
            installer.bundle("pesquisa", v1).exists() and installer.bundle("pesquisa", v2).exists()
        )
        restored = installer.rollback("pesquisa", reviewed_digest=v1, reviewed_capabilities=set())
        assert restored["selected_digest"] == v1 and restored["previous_digest"] == v2
        store.transition("pesquisa", PluginState.ACTIVE)
        with pytest.raises(PluginError, match="plugin_disable_first"):
            installer.uninstall("pesquisa")
        store.transition("pesquisa", PluginState.DISABLED)
        installer.uninstall("pesquisa")
        assert store.list() == [] and output.read_text() == "resultado importante"
        assert not list(installer.bundles.rglob("manifest.json"))
    finally:
        store.close()


def test_tampered_bundle_refused_before_version_selection(bundle, tmp_path):
    store = PluginStore(tmp_path / "registry.db")
    installer = Installer(tmp_path / "extensions", store)
    try:
        result = installer.install_folder(bundle)
        path = installer.root / result["bundle"] / "manifest.json"
        path.chmod(0o600)
        path.write_text(path.read_text().replace("Pesquisa", "Alterado"))
        with pytest.raises(PluginError, match="bundle_tampered"):
            installer.bundle("pesquisa", result["selected_digest"])
    finally:
        store.close()
