import asyncio
import base64
import hashlib
import json
import time
from urllib.parse import parse_qs, urlsplit

import httpx2
import pytest
from mcp.shared.auth import OAuthToken

from orion.accounts import Account, Accounts, Flow, Storage
from orion.extensions.host import HTTPConfig, MCPError
from orion.memory import MemoryStore


async def test_sdk_oauth_pkce_state_scope_expiry_refresh_revoke_no_plaintext(tmp_path):
    memory = MemoryStore(tmp_path / "memory.db")
    vault = {}
    a = Accounts(memory, get=vault.get, set_=vault.__setitem__)
    row = a.create(Account(name="Ensaio", url="https://mcp.example/mcp", scopes=["read"]))
    id_ = row["id"]
    revision = a.row(id_)["revision"]
    flow = Flow(revision, asyncio.get_running_loop().create_future(), time.time() + 10)
    a.flows[id_] = flow
    cfg = HTTPConfig(id="fixture", url=row["url"], account_id=id_)
    auth = a.provider(cfg, "http://127.0.0.1/accounts/callback/" + id_, flow)
    calls = []

    def endpoint(request):
        path = request.url.path
        calls.append((path, request.content))
        if request.url.host == "mcp.example" and path == "/mcp":
            if request.headers.get("authorization") in {
                "Bearer fixture-access",
                "Bearer refreshed-access",
            }:
                return httpx2.Response(200, json={"ok": True})
            return httpx2.Response(
                401,
                headers={
                    "WWW-Authenticate": 'Bearer resource_metadata="https://mcp.example/.well-known/oauth-protected-resource"'
                },
            )
        if "oauth-protected-resource" in path:
            return httpx2.Response(
                200,
                json={
                    "resource": "https://mcp.example/mcp",
                    "authorization_servers": ["https://auth.example"],
                    "scopes_supported": ["read"],
                },
            )
        if "/.well-known/" in path:
            return httpx2.Response(
                200,
                json={
                    "issuer": "https://auth.example",
                    "authorization_endpoint": "https://auth.example/authorize",
                    "token_endpoint": "https://auth.example/token",
                    "registration_endpoint": "https://auth.example/register",
                    "response_types_supported": ["code"],
                    "grant_types_supported": ["authorization_code", "refresh_token"],
                    "token_endpoint_auth_methods_supported": ["none"],
                    "code_challenge_methods_supported": ["S256"],
                    "scopes_supported": ["read"],
                },
            )
        if path == "/register":
            metadata = json.loads(request.content)
            return httpx2.Response(201, json={**metadata, "client_id": "fixture-client"})
        if path == "/token":
            form = parse_qs(request.content.decode())
            if form["grant_type"] == ["authorization_code"]:
                challenge = (
                    base64.urlsafe_b64encode(
                        hashlib.sha256(form["code_verifier"][0].encode()).digest()
                    )
                    .rstrip(b"=")
                    .decode()
                )
                assert challenge == parse_qs(urlsplit(flow.url).query)["code_challenge"][0]
                token = "fixture-access"
            else:
                assert form["grant_type"] == ["refresh_token"]
                assert form["refresh_token"] == ["fixture-refresh"]
                token = "refreshed-access"
            return httpx2.Response(
                200,
                json={
                    "access_token": token,
                    "refresh_token": "fixture-refresh",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "scope": "read",
                },
            )
        raise AssertionError(str(request.url))

    async with httpx2.AsyncClient(transport=httpx2.MockTransport(endpoint), auth=auth) as client:
        task = asyncio.create_task(client.get(cfg.url))
        async with asyncio.timeout(5):
            while not flow.url:  # noqa: ASYNC110 — fixture aguarda a URL produzida pelo SDK
                await asyncio.sleep(0.01)
        with pytest.raises(MCPError, match="oauth_callback_invalid"):
            a.callback(id_, "fixture-code", "wrong")
        a.callback(id_, "fixture-code", flow.state)
        assert (await task).json() == {"ok": True}
        with pytest.raises(MCPError):
            a.callback(id_, "fixture-code", flow.state)
    assert a.row(id_)["state"] == "authorized"
    storage = Storage(a, id_, revision)
    await storage.set_tokens(
        OAuthToken(
            access_token="fixture-access",
            refresh_token="fixture-refresh",
            expires_in=0,
            scope="read",
        )
    )
    assert (await storage.get_tokens()).expires_in == 0
    async with httpx2.AsyncClient(
        transport=httpx2.MockTransport(endpoint), auth=a.provider(cfg)
    ) as client:
        assert (await client.get(cfg.url)).json() == {"ok": True}
    assert any(b"refresh_token" in body for path, body in calls if path == "/token")
    with pytest.raises(MCPError, match="account_scope_exceeded"):
        await storage.set_tokens(OAuthToken(access_token="broad", scope="read write"))
    assert "fixture-access" not in str(
        [dict(r) for r in memory.query("SELECT * FROM oauth_accounts")]
    )
    await a.revoke(id_)
    with pytest.raises(MCPError, match="account_revoked"):
        await storage.get_tokens()
    with pytest.raises(MCPError, match="account_out_of_scope_or_revoked"):
        a.provider(cfg)
    memory.close()


def test_account_auth_callback_controls_cofre_unavailable_and_no_rest_token(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings
    from tests.projects.test_projects import AUTH, TOKEN

    with TestClient(
        create_app(
            Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
        ),
        base_url="http://127.0.0.1",
    ) as c:
        assert c.get("/accounts").status_code == 401
        assert (
            c.post(
                "/accounts",
                json={"name": "X", "url": "http://remote.example/mcp", "scopes": ["read"]},
                headers=AUTH,
            ).status_code
            == 422
        )
        row = c.post(
            "/accounts",
            json={"name": "X", "url": "https://mcp.example/mcp", "scopes": ["read"]},
            headers=AUTH,
        ).json()
        assert c.get(f"/accounts/callback/{row['id']}?code=fixture&state=wrong").status_code == 422

        def failed(*args):
            raise RuntimeError("No secure credential storage")

        c.app.state.orion.accounts.set = failed
        assert (
            c.post(f"/accounts/{row['id']}/authorize", headers=AUTH).json()["detail"]
            == "credential_store_unavailable"
        )
        assert c.post(f"/accounts/{row['id']}/revoke", headers=AUTH).json()["state"] == "revoked"
        assert "token" not in c.get("/accounts", headers=AUTH).text
