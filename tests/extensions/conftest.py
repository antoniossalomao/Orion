import json

import pytest


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
