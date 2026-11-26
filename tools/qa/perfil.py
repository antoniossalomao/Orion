"""Perfil de teste isolado, admin local gerado e chaves de API lidas de um .docx (só em memória)."""
from __future__ import annotations

import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

from qa_lib import RAIZ, RUST, ambiente_base, registrar_segredo

ADMIN_ARQ = RUST / ".orion-test-admin.local"
PERFIL = Path(__file__).parent / ".perfil"
PORTA = "18765"
ALIAS = {"claude": "anthropic", "google": "gemini", "chatgpt": "openai", "gpt": "openai"}


def admin() -> dict:
    """Lê/gera o admin de teste (qa-admin + senha aleatória >=24 chars). Nunca commitado; ACL só do usuário."""
    if ADMIN_ARQ.is_file():
        d = json.loads(ADMIN_ARQ.read_text(encoding="utf-8"))
    else:
        d = {"usuario": "qa-admin", "senha": secrets.token_urlsafe(24)}
        ADMIN_ARQ.write_text(json.dumps(d), encoding="utf-8")
        if sys.platform == "win32":  # só o usuário atual lê
            user = os.environ.get("USERNAME", "")
            subprocess.run(["icacls", str(ADMIN_ARQ), "/inheritance:r", "/grant:r", f"{user}:(R,W)"], capture_output=True, check=False)  # noqa: S603,S607
        else:
            ADMIN_ARQ.chmod(0o600)
    registrar_segredo(d["senha"])
    return d


def chaves_docx(caminho: str | None) -> dict[str, str]:
    """Extrai `NOME: valor` / `NOME=valor` dos parágrafos do .docx. Devolve {ORION_KEY_<ID>: valor}.

    Em memória apenas; os valores entram no mascarador. Nunca grava, nunca imprime.
    """
    if not caminho:
        return {}
    with zipfile.ZipFile(caminho) as z:
        xml = z.read("word/document.xml").decode("utf-8", "ignore")
    texto = re.sub(r"</w:p>", "\n", xml)
    texto = re.sub(r"<[^>]+>", "", texto)
    saida: dict[str, str] = {}
    for linha in texto.splitlines():
        m = re.match(r"^\s*([A-Za-z0-9_\- ]{2,40}?)\s*[:=]\s*(\S{16,})\s*$", linha)
        if not m:
            continue
        pid = re.sub(r"[^a-z0-9]+", "-", m.group(1).lower()).strip("-")
        pid = re.sub(r"-?(api-?)?(key|chave|token)$", "", pid) or pid
        pid = ALIAS.get(pid, pid)
        registrar_segredo(m.group(2))
        saida[f"ORION_KEY_{pid.upper().replace('-', '_')}"] = m.group(2)
    return saida


def exe(nome: str, pasta_bin: str | None) -> list[str]:
    """Binário `orion`/`orion-app`: --bin, senão target/release|debug, senão `cargo run`."""
    base = Path(pasta_bin) if pasta_bin else None
    for b in ([base] if base else []) + [RUST / "target" / "release", RUST / "target" / "debug"]:
        p = b / f"{nome}.exe"
        if p.is_file():
            return [str(p)]
    pacote = {"orion": "orion-server", "orion-app": "orion-app"}[nome]
    return ["cargo", "run", "-q", "-p", pacote, "--bin", nome, "--"]


def ambiente(chaves: dict[str, str], tag: str = "geral") -> dict:
    """Ambiente do app de teste: dados/vault isolados, porta própria, chaves só no env do processo."""
    pasta = PERFIL / tag
    for sub in ("dados", "vault", "runtime"):
        (pasta / sub).mkdir(parents=True, exist_ok=True)
    a = admin()
    env = ambiente_base()
    env.update(
        ORION_DATA_DIR=str(pasta / "dados"),
        ORION_VAULT_DIR=str(pasta / "vault"),
        ORION_RUNTIME_DIR=str(pasta / "runtime"),
        ORION_PORT=PORTA,
        ORION_URL=f"http://127.0.0.1:{PORTA}",
        ORION_USER=a["usuario"],
        ORION_AUTH_USER=a["usuario"],
        ORION_SEED_DEFAULT_PASSWORD="false",
    )
    env.update(chaves)
    return env


def definir_senha(env: dict, cmd_orion: list[str]) -> None:
    a = admin()
    r = subprocess.run([*cmd_orion, "set-password", "--stdin"], input=a["senha"] + "\n", env=env, capture_output=True, text=True, timeout=120)  # noqa: S603
    if r.returncode != 0:
        raise RuntimeError("set-password falhou (saída omitida por segurança)")


def limpar() -> None:
    shutil.rmtree(PERFIL, ignore_errors=True)


def limpar_tag(tag: str) -> None:
    shutil.rmtree(PERFIL / tag, ignore_errors=True)
