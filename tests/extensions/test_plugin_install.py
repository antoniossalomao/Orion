import json
import os

import pytest

from orion.extensions.installer import Installer, folder_snapshot
from orion.extensions.plugins import PluginError, PluginStore


@pytest.fixture
def bundle(tmp_path):
    root = tmp_path / "source"
    skill = root / "skills/revisar"
    (skill / "scripts").mkdir(parents=True)
    (root / "manifest.json").write_text(
        json.dumps(
            {
                "id": "pesquisa",
                "version": "1.0.0",
                "name": "Pesquisa",
                "description": "Revisar fontes",
                "license": "MIT",
                "skills": ["skills/revisar"],
            }
        )
    )
    (skill / "SKILL.md").write_text(
        "---\nname: revisar\ndescription: Revisar fontes\n---\n[script](scripts/perigo.py)"
    )
    (skill / "scripts/perigo.py").write_text(
        "from pathlib import Path; Path(__file__).with_suffix('.ran').touch()"
    )
    return root


@pytest.fixture
def installer(tmp_path):
    store = PluginStore(tmp_path / "data/plugins.db")
    yield Installer(tmp_path / "data/extensions", store)
    store.close()


def test_install_disabled_immutable_copy_no_code_and_idempotent(bundle, installer):
    result = installer.install_folder(bundle)
    assert result["state"] == "disabled" and result["active_digest"] is None
    target = installer.root / result["bundle"]
    assert target.exists() and not list(target.rglob("*.ran"))
    assert folder_snapshot(target).digest == result["selected_digest"]
    (bundle / "skills/revisar/SKILL.md").write_text("origem mudou após instalar")
    assert "origem mudou" not in (target / "skills/revisar/SKILL.md").read_text()
    assert not list(installer.staging.iterdir())
    result2 = installer.install_folder(target)
    assert result2["selected_digest"] == result["selected_digest"]
    if os.name == "posix":
        assert not (target / "manifest.json").stat().st_mode & 0o222


@pytest.mark.parametrize("kind", ["symlink", "traversal", "secret", "collision", "hardlink"])
def test_unsafe_bundle_never_installed(bundle, installer, tmp_path, kind):
    if kind == "symlink":
        (bundle / "escape").symlink_to(tmp_path)
    elif kind == "traversal":
        (bundle / "skills/revisar/SKILL.md").write_text(
            "---\nname: revisar\ndescription: x\n---\n[fora](../../../fora)"
        )
    elif kind == "secret":
        (bundle / ".env").write_text("segredo de ensaio")
    elif kind == "collision":
        (bundle / "Manifest.json").write_text("{}")
    else:
        file = tmp_path / "fora"
        file.write_text("fora")
        os.link(file, bundle / "linked")
    with pytest.raises(PluginError):
        installer.install_folder(bundle)
    assert installer.store.list() == [] and not list(installer.staging.iterdir())


def test_interrupted_install_rolls_back_objects_and_database(bundle, installer, monkeypatch):
    def fail(*args, **kwargs):
        raise PluginError("fixture_interrupted")

    monkeypatch.setattr(installer.store, "record", fail)
    with pytest.raises(PluginError, match="fixture_interrupted"):
        installer.install_folder(bundle)
    assert installer.store.list() == [] and not list(installer.staging.iterdir())
    assert not list(installer.bundles.rglob("manifest.json"))
