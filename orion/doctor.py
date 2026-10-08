"""`orion doctor`: confere a instalação num comando, sem rede e sem imprimir segredo.

Cada checagem devolve (nível, o que viu). Nível `erro` impede o uso; `aviso` é algo a
resolver; `ok` está certo. Só os NOMES das chaves aparecem, nunca o valor.
"""

from __future__ import annotations

import sqlite3
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from .config import Settings
from .secrets import get_secret

Nivel = Literal["ok", "aviso", "erro"]


@dataclass(frozen=True)
class Checagem:
    nome: str
    nivel: Nivel
    detalhe: str


def _chave(settings: Settings, campo: str, cofre: str) -> bool:
    return bool(getattr(settings, campo, "") or get_secret(cofre))


def _dados(s: Settings) -> Checagem:
    try:
        s.data_dir.mkdir(parents=True, exist_ok=True)
        teste = s.data_dir / ".doctor"
        teste.write_text("x")
        teste.unlink()
    except OSError as e:
        return Checagem("pasta de dados", "erro", f"{s.data_dir}: {type(e).__name__}")
    return Checagem("pasta de dados", "ok", str(s.data_dir))


def _banco(s: Settings) -> Checagem:
    from .memory.schema import SCHEMA_VERSION

    if not s.db_path.exists():
        return Checagem("banco da memória", "aviso", "ainda não existe (nasce no primeiro uso)")
    try:
        con = sqlite3.connect(f"file:{s.db_path}?mode=ro", uri=True)
        try:
            v = int(con.execute("SELECT value FROM meta WHERE key='schema_version'").fetchone()[0])
            ok = con.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        finally:
            con.close()
    except (sqlite3.Error, TypeError, ValueError) as e:
        return Checagem("banco da memória", "erro", f"ilegível: {type(e).__name__}")
    if not ok:
        return Checagem("banco da memória", "erro", "integrity_check falhou: restaure um backup")
    if v > SCHEMA_VERSION:
        return Checagem("banco da memória", "erro", f"esquema v{v} é mais novo que este Orion")
    return Checagem("banco da memória", "ok", f"esquema v{v}, íntegro")


def _login(s: Settings) -> Checagem:
    from .auth import AuthService

    if not s.auth_db_path.exists() and not s.admin_token:
        return Checagem("login", "aviso", "sem senha nem token: rode `orion set-password`")
    if not s.auth_db_path.exists():
        return Checagem("login", "ok", "só pelo ORION_ADMIN_TOKEN (sem senha)")
    auth = AuthService(s.auth_db_path, user=s.auth_user)
    try:
        if not auth.has_password():
            return Checagem("login", "aviso", "sem senha definida: `orion set-password`")
        if auth.uses_default_password():
            return Checagem("login", "erro", "ainda é a senha de fábrica: troque com set-password")
    finally:
        auth.close()
    return Checagem("login", "ok", "senha definida")


def _rede(s: Settings) -> Checagem:
    if s.hosts_de_fora and not (s.admin_token or s.auth_db_path.exists()):
        return Checagem("acesso de fora", "erro", "host liberado sem login (regra 17)")
    if s.host not in ("127.0.0.1", "localhost", "::1") and not s.allow_public_bind:
        return Checagem("acesso de fora", "erro", f"host {s.host} sem allow_public_bind")
    return Checagem("acesso de fora", "ok", "só local" if not s.hosts_de_fora else "com login")


def _gateway(s: Settings) -> Checagem:
    if not s.gateway_url:
        return Checagem("gateway de modelos", "aviso", "ORION_GATEWAY_URL vazio: /chat desligado")
    if not _chave(s, "gateway_api_key", "ORION_GATEWAY_API_KEY"):
        return Checagem("gateway de modelos", "aviso", "sem ORION_GATEWAY_API_KEY")
    return Checagem("gateway de modelos", "ok", f"{s.gateway_url} (não testei a rede)")


def _opcionais(s: Settings) -> list[Checagem]:
    out: list[Checagem] = []
    precisa: list[tuple[str, bool, str, str, str]] = [
        ("embeddings", True, "embed_api_key", "ORION_EMBED_API_KEY", "busca só por texto (FTS)"),
        (
            "voz (Groq)",
            s.voice_enabled,
            "transcribe_api_key",
            "ORION_TRANSCRIBE_API_KEY",
            "voz ligada sem chave de transcrição",
        ),
        (
            "Telegram",
            bool(s.telegram_allowed_users),
            "telegram_token",
            "ORION_TELEGRAM_TOKEN",
            "usuários liberados mas sem token",
        ),
    ]
    for nome, ligado, campo, cofre, falta in precisa:
        if _chave(s, campo, cofre):
            out.append(Checagem(nome, "ok", "chave presente"))
        elif ligado:
            out.append(Checagem(nome, "aviso", falta))
    if s.research_at and not (s.web_tools and s.vault_dir):
        out.append(
            Checagem("pesquisa noturna", "aviso", "ORION_RESEARCH_AT sem ORION_WEB_TOOLS e vault")
        )
    if s.wake_enabled and not s.voice_enabled:
        out.append(Checagem("palavra de ativação", "aviso", "wake ligado sem ORION_VOICE_ENABLED"))
    return out


def _mcp(s: Settings) -> Checagem:
    caminho = s.effective_mcp_config
    if not s.mcp_enabled:
        return Checagem("MCP", "ok", "desligado")
    if not caminho.exists():
        return Checagem("MCP", "aviso", f"sem {caminho.name} (copie mcp.example.json)")
    import json

    try:
        cfg = json.loads(caminho.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        return Checagem("MCP", "erro", f"{caminho.name} inválido: {type(e).__name__}")
    n = len(cfg.get("servers", cfg.get("mcpServers", {}))) if isinstance(cfg, dict) else 0
    return Checagem("MCP", "ok", f"{n} servidor(es) em {caminho.name} (rode `mcp-check`)")


def _backup(s: Settings) -> Checagem:
    pasta = s.effective_backup_dir
    dbs = sorted(pasta.glob("*.db"), key=lambda p: p.stat().st_mtime) if pasta.exists() else []
    if not dbs:
        return Checagem("backup", "aviso", f"nenhum backup em {pasta} (`orion backup`)")
    import time

    dias = (time.time() - dbs[-1].stat().st_mtime) / 86400
    nivel: Nivel = "ok" if dias < 3 else "aviso"
    return Checagem("backup", nivel, f"último há {dias:.0f} dia(s), {len(dbs)} guardado(s)")


def _vault(s: Settings) -> Checagem:
    if s.vault_dir is None:
        return Checagem("vault", "aviso", "ORION_VAULT_DIR vazio: notas não entram na memória")
    if not Path(s.vault_dir).is_dir():
        return Checagem("vault", "erro", f"{s.vault_dir} não é uma pasta")
    return Checagem("vault", "ok", str(s.vault_dir))


def checar(s: Settings) -> list[Checagem]:
    fixas: list[Callable[[Settings], Checagem]] = [
        _dados, _banco, _login, _rede, _gateway, _mcp, _backup, _vault,
    ]  # fmt: skip
    return [f(s) for f in fixas] + _opcionais(s)


def relatorio(itens: list[Checagem]) -> tuple[str, int]:
    marca = {"ok": "ok   ", "aviso": "aviso", "erro": "ERRO "}
    linhas = [f"[{marca[i.nivel]}] {i.nome}: {i.detalhe}" for i in itens]
    erros = sum(i.nivel == "erro" for i in itens)
    avisos = sum(i.nivel == "aviso" for i in itens)
    linhas.append(f"\n{erros} erro(s), {avisos} aviso(s)")
    return "\n".join(linhas), 1 if erros else 0


__all__ = ["Checagem", "checar", "relatorio"]
