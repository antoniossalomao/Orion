import json

import pytest

from orion.extensions.grants import Grants
from orion.extensions.installer import Installer
from orion.extensions.plugins import PluginError, PluginStore
from orion.extensions.skill_runtime import SkillRuntime, SkillSource
from orion.policy import Action, Context, PolicyEngine, Risk, ToolCall, ToolSpec
from orion.policy.paths import PathGuard


def test_revision_scope_and_approved_native_call_are_revoked(bundle, tmp_path):
    manifest = json.loads((bundle / "manifest.json").read_text())
    manifest["capabilities"] = ["native:write", "native:read"]
    (bundle / "manifest.json").write_text(json.dumps(manifest))
    store = PluginStore(tmp_path / "plugins.db")
    installer = Installer(tmp_path / "installed", store)
    current = installer.install_folder(bundle)
    grants = Grants(store)
    digest = current["selected_digest"]
    assert grants.effective("pesquisa", digest) == set()
    with pytest.raises(PluginError, match="grant_invalid"):
        grants.review("pesquisa", digest, {"scripts"})
    grants.review("pesquisa", digest, {"native:write"}, scope="project-a")
    assert grants.effective("pesquisa", digest, scope="project-b") == set()
    assert grants.effective("pesquisa", digest, scope="project-a", allowed={"native:read"}) == set()
    runtime = SkillRuntime(
        [
            SkillSource(
                root=installer.bundle("pesquisa", digest) / "skills",
                namespace="pesquisa",
                enabled=True,
            )
        ]
    )
    runtime.scopes["pesquisa:revisar"] = frozenset({"write"})
    runtime.authorities["pesquisa:revisar"] = f"pesquisa:{digest}:generation1"
    chosen = runtime.select("revisar", ["pesquisa:revisar"])
    policy = PolicyEngine(
        path_guard=PathGuard(),
        tools={"write": ToolSpec("write", Risk.EXEC), "read": ToolSpec("read", Risk.READ)},
    )
    ctx = Context(
        "sessao",
        allowed_tools=chosen.allowed_tools,
        authorities=chosen.authorities,
        authorized=chosen.authorized,
        project_id="a",
    )
    call = ToolCall("write", {"value": 1})
    pending = policy.evaluate(call, ctx)
    assert pending.action == Action.CONFIRM
    policy.approvals.decide(pending.approval_id, True, channel="web", actor="fixture")
    assert policy.evaluate(ToolCall("read"), ctx).action == Action.DENY
    # Mesmo argumento/sessão, mas outro projeto ou revisão: decisão anterior não serve.
    other = Context(
        "sessao", allowed_tools=chosen.allowed_tools, authorities=chosen.authorities, project_id="b"
    )
    assert policy.evaluate(call, other).action == Action.CONFIRM
    runtime.enabled.clear()
    policy.approvals.invalidate_binding(chosen.authorities[0])
    assert policy.evaluate(call, ctx).action == Action.DENY
    assert policy.approvals.get(pending.approval_id).status == "denied"
    grants.revoke("pesquisa")
    assert not grants.effective("pesquisa", digest, scope="project-a")
    store.close()
