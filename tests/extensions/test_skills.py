import pytest

from orion.extensions.skills import SkillError, SkillIndex, metadata


def package(tmp_path, header="name: ensaio\ndescription: descrição válida", body="Corpo."):
    root = tmp_path / "ensaio"
    root.mkdir(exist_ok=True)
    (root / "SKILL.md").write_text(f"---\n{header}\n---\n{body}", encoding="utf-8")
    return root


def test_metadata_discovery_body_lazy_references_and_no_scripts(tmp_path):
    root = package(
        tmp_path,
        header='name: ensaio\ndescription: descrição válida\nmetadata:\n  version: "1.0"',
        body="Leia [manual](references/manual.md).\nLink [web](https://example.org/).",
    )
    (root / "references").mkdir()
    (root / "references/manual.md").write_text("manual")
    (root / "scripts").mkdir()
    (root / "scripts/perigo.py").write_text(
        "from pathlib import Path; Path(__file__).with_suffix('.ran').touch()"
    )
    skill = metadata(root, namespace="pesquisa", origin="fixture")
    assert skill.summary()["id"] == "pesquisa:ensaio" and skill.version == "1.0"
    assert "text" not in skill.summary()
    body = skill.load()
    assert "references/manual.md" in body.references
    assert skill.reference("references/manual.md") == "manual"
    assert not (root / "scripts/perigo.ran").exists()
    index = SkillIndex()
    index.discover(tmp_path, namespace="pesquisa")
    assert len(index.summaries()) == 1
    with pytest.raises(SkillError, match="skill_name_collision"):
        index.add(skill)


@pytest.mark.parametrize(
    "header",
    [
        "name: ENSAIO\ndescription: x",
        "name: ensaio\ndescription: ''",
        "name: ensaio\ndescription: x\nname: outra",
        "name: ensaio\ndescription: &a abc\nlicense: *a",
        "name: ensaio\ndescription: !!python/object/apply:os.system ['echo x']",
        "name: ensaio\ndescription: [x]",
        "name: outro\ndescription: x",
    ],
)
def test_invalid_frontmatter_rejected(tmp_path, header):
    with pytest.raises(SkillError):
        metadata(package(tmp_path, header), namespace="fixture")


@pytest.mark.parametrize(
    "link",
    [
        "../../fora.md",
        "/etc/passwd",
        "C:/segredo",
        "%2e%2e/fora",
        "file:///etc/passwd",
        "references/link.md",
    ],
)
def test_reference_escape_rejected(tmp_path, link):
    root = package(tmp_path, body=f"[referência]({link})")
    (root / "references").mkdir()
    (root / "references/link.md").symlink_to(tmp_path / "fora.md")
    (tmp_path / "fora.md").write_text("fora")
    with pytest.raises(SkillError):
        metadata(root, namespace="fixture").load()


def test_metadata_does_not_read_invalid_body_until_selected_and_detects_change(tmp_path):
    root = package(tmp_path)
    path = root / "SKILL.md"
    path.write_bytes(path.read_bytes() + b"\xff")
    skill = metadata(root, namespace="fixture")
    assert skill.header.description == "descrição válida"
    with pytest.raises(SkillError, match="skill_encoding_invalid"):
        skill.load()
    package(tmp_path, body="nova versão")
    skill = metadata(root, namespace="fixture")
    package(tmp_path, header="name: ensaio\ndescription: modificado")
    with pytest.raises(SkillError, match="skill_changed"):
        skill.load()
