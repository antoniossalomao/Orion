import hashlib
import json

import pytest

from orion.extensions.plugins import PluginError, PluginState, PluginStore, parse_manifest


def manifest(**updates):
    value = {
        "id": "pesquisa",
        "version": "1.0.0",
        "name": "Pesquisa",
        "description": "Pesquisar fontes",
        "license": "MIT",
        "skills": ["skills/revisar"],
        "capabilities": ["native:buscar_memoria"],
    }
    value.update(updates)
    return json.dumps(value).encode()


def test_skill_only_mcp_manifest_namespaces_refs_and_persistence(tmp_path):
    skill_only = parse_manifest(manifest())
    assert skill_only.skills == ["skills/revisar"]
    mixed = parse_manifest(
        manifest(
            mcp=[
                {
                    "id": "fontes",
                    "transport": "http",
                    "url": "https://example.org/mcp",
                    "secret_ref": "ORION_MCP_PESQUISA",
                    "tools": {"buscar": "read"},
                }
            ],
            capabilities=["mcp:fontes:buscar"],
        )
    )
    assert "ORION_MCP_PESQUISA" in mixed.model_dump_json()
    digest = hashlib.sha256(manifest()).hexdigest()
    store = PluginStore(tmp_path / "plugins.db")
    store.record(skill_only, digest, "fixture")
    store.close()
    store = PluginStore(tmp_path / "plugins.db")
    try:
        record = store.get("pesquisa")
        assert record["state"] == "disabled" and record["selected_digest"] == digest
        store.record(skill_only, digest, "fixture")  # idempotente
        for state in PluginState:
            store.transition("pesquisa", state)
            assert store.get("pesquisa")["state"] == state.value
        with pytest.raises(PluginError, match="plugin_version_conflict"):
            store.record(skill_only, "f" * 64, "fixture")
        with pytest.raises(PluginError, match="plugin_id_conflict"):
            store.record(parse_manifest(manifest(version="2.0.0")), "a" * 64, "outro")
    finally:
        store.close()


@pytest.mark.parametrize(
    "updates",
    [
        {"schema_version": 2},
        {"orion_api": 2},
        {"id": "../../outro"},
        {"version": "latest"},
        {"skills": ["../fora"]},
        {"skills": ["skills/revisar", "skills/revisar"]},
        {
            "mcp": [
                {"id": "x", "transport": "http", "url": "https://example.org/mcp", "token": "valor"}
            ]
        },
        {"mcp": [{"id": "x", "transport": "stdio", "entrypoint": "../../script.py"}]},
        {"mcp": [{"id": "x", "transport": "http", "url": "https://token:x@example.org/mcp"}]},
        {"capabilities": ["*", "admin:approve"]},
        {"dependencies": [{"id": "pesquisa", "version": "1.0.0"}]},
        {"install_hooks": ["pip install malware"]},
    ],
)
def test_invalid_manifests_do_not_execute_or_grant(updates):
    with pytest.raises(PluginError):
        parse_manifest(manifest(**updates))


def test_duplicate_json_key_rejected():
    with pytest.raises(PluginError, match="manifest_duplicate_key"):
        parse_manifest(b'{"id":"a", "id":"b"}')
