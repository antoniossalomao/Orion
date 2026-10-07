"""Contas OAuth MCP: metadados no SQLite, credenciais somente no cofre do SO."""

from __future__ import annotations

import asyncio
import json
import logging
import secrets
import time
import uuid
from dataclasses import dataclass
from urllib.parse import parse_qs, urlsplit

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from mcp.client.auth.oauth2 import OAuthClientProvider
from mcp.shared.auth import (
    AuthorizationCodeResult,
    OAuthClientInformationFull,
    OAuthClientMetadata,
    OAuthMetadata,
    OAuthToken,
)
from pydantic import BaseModel, ConfigDict, Field

from .extensions.host import HTTPConfig, MCPError, MCPHost
from .secrets import get_secret, set_secret


class Account(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=160)
    url: str = Field(max_length=4096)
    scope: str = Field(default="personal", pattern=r"^(personal|project:[a-f0-9]{32})$")
    scopes: list[str] = Field(min_length=1, max_length=32)


@dataclass
class Flow:
    revision: str
    future: asyncio.Future
    expires: float
    url: str | None = None
    state: str | None = None
    used: bool = False
    task: asyncio.Task | None = None


class CallbackLogFilter(logging.Filter):
    def filter(self, record):
        if isinstance(record.args, tuple):
            record.args = tuple(
                a.split("?", 1)[0] if isinstance(a, str) and "/accounts/callback/" in a else a
                for a in record.args
            )
        return True


class Storage:
    def __init__(self, accounts, id_, revision):
        self.accounts, self.id, self.revision = accounts, id_, revision

    def valid(self):
        row = self.accounts.row(self.id)
        if row["revision"] != self.revision or row["state"] == "revoked":
            raise MCPError("account_revoked")
        return row

    def key(self, kind):
        return f"ORION_ACCOUNT_{self.id.upper()}_{self.revision.upper()}_{kind}"

    async def get_tokens(self):
        self.valid()
        raw = await asyncio.to_thread(self.accounts.get, self.key("TOKENS"))
        if not raw:
            return None
        record = json.loads(raw)
        token = OAuthToken.model_validate(record["token"])
        if record.get("expires_at") is not None:
            token.expires_in = max(0, int(record["expires_at"] - time.time()))
        return token

    async def set_tokens(self, tokens: OAuthToken):
        row = self.valid()
        allowed = set(json.loads(row["scopes"]))
        granted = set((tokens.scope or "").split())
        if granted - allowed:
            raise MCPError("account_scope_exceeded")
        expiry = time.time() + tokens.expires_in if tokens.expires_in is not None else None
        await asyncio.to_thread(
            self.accounts.set,
            self.key("TOKENS"),
            json.dumps({"token": tokens.model_dump(), "expires_at": expiry}),
        )
        self.valid()
        with self.accounts.memory.transaction() as c:
            c.execute(
                "UPDATE oauth_accounts SET "
                "state='authorized',expires_at=?,granted=?,error=NULL WHERE id=? AND revision=?",
                (expiry, json.dumps(sorted(granted)), self.id, self.revision),
            )

    async def get_client_info(self):
        self.valid()
        raw = await asyncio.to_thread(self.accounts.get, self.key("CLIENT"))
        return OAuthClientInformationFull.model_validate_json(raw) if raw else None

    async def set_client_info(self, client_info):
        self.valid()
        await asyncio.to_thread(
            self.accounts.set, self.key("CLIENT"), client_info.model_dump_json()
        )


class PersistedOAuthProvider(OAuthClientProvider):
    """SDK 2.3: reconstruir prazo e issuer na recarga do armazenamento."""

    async def _initialize(self):
        await super()._initialize()
        storage = self.context.storage
        if not isinstance(storage, Storage):
            raise MCPError("account_storage_invalid")
        raw = await asyncio.to_thread(storage.accounts.get, storage.key("METADATA"))
        if raw:
            self.context.oauth_metadata = OAuthMetadata.model_validate_json(raw)
        token = self.context.current_tokens
        if token:
            self.context.update_token_expiry(token)
            if token.expires_in == 0:
                self.context.token_expiry_time = time.time() - 1

    async def _handle_token_response(self, response):
        await super()._handle_token_response(response)
        storage = self.context.storage
        if isinstance(storage, Storage) and self.context.oauth_metadata:
            storage.valid()
            await asyncio.to_thread(
                storage.accounts.set,
                storage.key("METADATA"),
                self.context.oauth_metadata.model_dump_json(),
            )


class Accounts:
    def __init__(self, memory, *, get=get_secret, set_=set_secret):
        self.memory, self.get, self.set = memory, get, set_
        self.flows: dict[str, Flow] = {}
        self.host: MCPHost | None = None
        logging.getLogger("uvicorn.access").addFilter(CallbackLogFilter())

    def row(self, id_):
        rows = self.memory.query("SELECT * FROM oauth_accounts WHERE id=?", (id_,))
        if not rows:
            raise MCPError("account_not_found")
        return dict(rows[0])

    def public(self, row):
        flow = self.flows.get(row["id"])
        return {
            **{k: row[k] for k in ("id", "name", "url", "scope", "state", "expires_at", "error")},
            "scopes": json.loads(row["scopes"]),
            "granted": json.loads(row["granted"]),
            "authorization_url": flow.url
            if flow and not flow.used and flow.expires > time.time()
            else None,
        }

    def create(self, body: Account):
        try:
            HTTPConfig(id="validate", url=body.url)
        except ValueError:
            raise MCPError("account_endpoint_invalid") from None
        if any(
            not scope or len(scope) > 256 or any(ord(c) < 33 for c in scope)
            for scope in body.scopes
        ):
            raise MCPError("account_scope_invalid")
        if self.memory.query("SELECT count(*) FROM oauth_accounts")[0][0] >= 32:
            raise MCPError("account_limit")
        id_ = uuid.uuid4().hex
        with self.memory.transaction() as c:
            c.execute(
                "INSERT INTO oauth_accounts(id,name,url,scope,scopes,granted,revision,state) "
                "VALUES (?,?,?,?,?,?,?,?)",
                (
                    id_,
                    body.name,
                    body.url,
                    body.scope,
                    json.dumps(sorted(set(body.scopes))),
                    "[]",
                    uuid.uuid4().hex,
                    "unauthorized",
                ),
            )
        return self.public(self.row(id_))

    def provider(self, config: HTTPConfig, callback_url=None, flow=None):
        row = self.row(config.account_id)
        if row["url"] != config.url or row["scope"] != config.scope or row["state"] == "revoked":
            raise MCPError("account_out_of_scope_or_revoked")
        revision = row["revision"]

        async def redirect(url):
            if not flow:
                raise MCPError("account_authorization_required")
            query = parse_qs(urlsplit(url).query)
            granted = set(query.get("scope", [""])[0].split())
            if granted - set(json.loads(row["scopes"])):
                raise MCPError("account_scope_exceeded")
            endpoint = urlsplit(url)
            if endpoint.scheme != "https" and not (
                endpoint.scheme == "http" and endpoint.hostname in {"127.0.0.1", "localhost", "::1"}
            ):
                raise MCPError("account_authorization_url_invalid")
            flow.state = query.get("state", [None])[0]
            flow.url = url

        async def callback():
            if not flow:
                raise MCPError("account_authorization_required")
            return await asyncio.wait_for(
                asyncio.shield(flow.future), max(0, flow.expires - time.time())
            )

        metadata = OAuthClientMetadata.model_validate(
            {
                "client_name": "Orion",
                "redirect_uris": [
                    callback_url or f"http://127.0.0.1/accounts/callback/{row['id']}"
                ],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
                "scope": " ".join(json.loads(row["scopes"])),
            }
        )
        return PersistedOAuthProvider(
            config.url, metadata, Storage(self, row["id"], revision), redirect, callback
        )

    async def authorize(self, id_, callback_url):
        endpoint = urlsplit(callback_url)
        if (
            endpoint.scheme != "http"
            or endpoint.hostname not in {"localhost", "127.0.0.1", "::1"}
            or endpoint.path != f"/accounts/callback/{id_}"
            or endpoint.query
            or endpoint.fragment
        ):
            raise MCPError("oauth_callback_url_invalid")
        await self.revoke(id_, disable_only=True)
        row = self.row(id_)
        try:
            await asyncio.to_thread(
                self.set, f"ORION_ACCOUNT_{id_.upper()}_PROBE", "cofre-validado"
            )
        except Exception as error:
            raise MCPError("credential_store_unavailable") from error
        with self.memory.transaction() as c:
            c.execute("UPDATE oauth_accounts SET state='authorizing',error=NULL WHERE id=?", (id_,))
        flow = Flow(row["revision"], asyncio.get_running_loop().create_future(), time.time() + 180)
        self.flows[id_] = flow

        async def run():
            from .extensions.host import Connection

            config = HTTPConfig(
                id="oauth-probe",
                url=row["url"],
                scope=row["scope"],
                account_id=id_,
                enabled=True,
                authorized=True,
            )
            probe = Connection(
                config,
                timeout=185,
                oauth_factory=lambda cfg: self.provider(cfg, callback_url, flow),
            )
            try:
                await probe.start()
                if probe.state != "connected":
                    with self.memory.transaction() as c:
                        c.execute(
                            "UPDATE oauth_accounts SET state='unauthorized',"
                            "error='authorization_failed' "
                            "WHERE id=? AND revision=?",
                            (id_, flow.revision),
                        )
            finally:
                flow.used = True
                await probe.close()

        flow.task = asyncio.create_task(run())
        return self.public(self.row(id_))

    def callback(self, id_, code, state, iss=None):
        flow = self.flows.get(id_)
        if (
            not flow
            or flow.used
            or flow.expires <= time.time()
            or not flow.state
            or not secrets.compare_digest(flow.state, state)
            or not code
            or len(code) > 4096
            or flow.revision != self.row(id_)["revision"]
        ):
            raise MCPError("oauth_callback_invalid")
        flow.used = True
        flow.future.set_result(AuthorizationCodeResult(code=code, state=state, iss=iss))

    async def revoke(self, id_, *, disable_only=False):
        row = self.row(id_)
        flow = self.flows.pop(id_, None)
        if flow and flow.task:
            flow.task.cancel()
            await asyncio.gather(flow.task, return_exceptions=True)
        with self.memory.transaction() as c:
            c.execute(
                "UPDATE oauth_accounts SET revision=?,state='revoked',error=NULL WHERE id=?",
                (uuid.uuid4().hex, id_),
            )
        if self.host:
            for conn in list(self.host.connections.values()):
                if isinstance(conn.config, HTTPConfig) and conn.config.account_id == id_:
                    await conn.close()
        if disable_only:
            with self.memory.transaction() as c:
                c.execute("UPDATE oauth_accounts SET state='unauthorized' WHERE id=?", (id_,))
        else:
            for kind in ("TOKENS", "CLIENT", "METADATA"):
                try:
                    await asyncio.to_thread(
                        self.set,
                        f"ORION_ACCOUNT_{id_.upper()}_{row['revision'].upper()}_{kind}",
                        "",
                    )
                except Exception:  # noqa: BLE001, S110 — identidade revogada antes de limpar o cofre
                    pass
        return self.public(self.row(id_))

    async def close(self):
        tasks = [f.task for f in self.flows.values() if f.task]
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


def router(require_admin):
    api = APIRouter()
    admin = [Depends(require_admin)]

    @api.get("/accounts", dependencies=admin)
    def listing(request: Request):
        a = request.app.state.orion.accounts
        return [a.public(dict(r)) for r in a.memory.query("SELECT * FROM oauth_accounts")]

    @api.post("/accounts", dependencies=admin)
    def create(body: Account, request: Request):
        if body.scope.startswith("project:"):
            rows = request.app.state.orion.memory.query(
                "SELECT archived FROM projects WHERE id=?", (body.scope[8:],)
            )
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_unavailable")
        return request.app.state.orion.accounts.create(body)

    @api.post("/accounts/{id_}/authorize", dependencies=admin)
    async def authorize(id_: str, request: Request):
        return await request.app.state.orion.accounts.authorize(
            id_, str(request.base_url).rstrip("/") + f"/accounts/callback/{id_}"
        )

    @api.post("/accounts/{id_}/revoke", dependencies=admin)
    async def revoke(id_: str, request: Request):
        s = request.app.state.orion
        result = await s.accounts.revoke(id_)
        await s.catalog.refresh()
        return result

    @api.get("/accounts/callback/{id_}")
    async def callback(
        id_: str, request: Request, code: str = "", state: str = "", iss: str | None = None
    ):
        request.app.state.orion.accounts.callback(id_, code, state, iss)
        return Response(
            "Autorização recebida. Volte ao Orion.",
            media_type="text/plain",
            headers={"Cache-Control": "no-store", "Referrer-Policy": "no-referrer"},
        )

    return api
