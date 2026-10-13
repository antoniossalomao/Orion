"""E9.5 — "Posso desligar?": confere o que ficaria pela metade se o computador desligasse agora.

Só lê: repositórios em `ORION_REPOS` com mudança não commitada ou commit não enviado, tarefa em
segundo plano rodando, backup do dia e downloads em andamento. Nada é corrigido aqui."""

import subprocess
import time
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

from .downloads import PARCIAIS

Runner = Callable[..., Any]
TIMEOUT_GIT_S = 10


def _git(runner: Runner, raiz: Path, *args: str) -> str | None:
    try:
        r = runner(
            ["git", "-C", str(raiz), *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=TIMEOUT_GIT_S,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout if r.returncode == 0 else None


def conferir_repo(raiz: Path, runner: Runner = subprocess.run) -> dict[str, Any]:
    nome = raiz.name or str(raiz)
    if not (raiz / ".git").exists():
        return {"id": f"repo:{nome}", "ok": True, "texto": f"{nome}: não é um repositório Git"}
    sujo = _git(runner, raiz, "status", "--porcelain")
    if sujo is None:
        return {"id": f"repo:{nome}", "ok": False, "texto": f"{nome}: não consegui consultar o Git"}
    # commits que não estão em nenhum remoto (cobre branch sem upstream também)
    local = _git(runner, raiz, "log", "--branches", "--not", "--remotes", "--oneline")
    mudancas = len([x for x in sujo.splitlines() if x.strip()])
    nao_enviados = len([x for x in (local or "").splitlines() if x.strip()])
    partes = []
    if mudancas:
        partes.append(f"{mudancas} arquivo(s) com mudança não commitada")
    if nao_enviados:
        partes.append(f"{nao_enviados} commit(s) não enviado(s)")
    if partes:
        return {"id": f"repo:{nome}", "ok": False, "texto": f"{nome}: " + " e ".join(partes)}
    return {"id": f"repo:{nome}", "ok": True, "texto": f"{nome}: tudo commitado e enviado"}


def conferir_downloads(pasta: Path) -> dict[str, Any]:
    try:
        vivos = sorted(
            p.name for p in pasta.iterdir() if p.is_file() and p.suffix.lower() in PARCIAIS
        )
    except OSError:
        vivos = []
    if vivos:
        return {
            "id": "downloads",
            "ok": False,
            "texto": f"{len(vivos)} download(s) em andamento: " + ", ".join(vivos[:3]),
        }
    return {"id": "downloads", "ok": True, "texto": "nenhum download em andamento"}


def conferir_backup(pasta: Path | None, agora: float) -> dict[str, Any]:
    if pasta is None:
        return {"id": "backup", "ok": True, "texto": "backup não configurado"}
    hoje = datetime.fromtimestamp(agora).strftime("%Y%m%d")
    if list(pasta.glob(f"orion-{hoje}-*.db")):
        return {"id": "backup", "ok": True, "texto": "backup de hoje feito"}
    return {"id": "backup", "ok": False, "texto": "o backup de hoje ainda não foi feito"}


def conferir_tarefas(tarefas: list[dict[str, Any]]) -> dict[str, Any]:
    rodando = [t for t in tarefas if t.get("status") == "rodando"]
    if rodando:
        objetivo = str(rodando[0].get("objective", ""))[:60]
        return {"id": "tarefa_bg", "ok": False, "texto": f"tarefa em segundo plano: {objetivo}"}
    return {"id": "tarefa_bg", "ok": True, "texto": "nenhuma tarefa em segundo plano"}


def posso_desligar(
    *,
    repos: list[Path],
    backup_dir: Path | None,
    downloads_dir: Path,
    tarefas: list[dict[str, Any]],
    runner: Runner = subprocess.run,
    clock: Callable[[], float] = time.time,
) -> dict[str, Any]:
    itens = [conferir_repo(r, runner) for r in repos]
    itens += [
        conferir_tarefas(tarefas),
        conferir_backup(backup_dir, clock()),
        conferir_downloads(downloads_dir),
    ]
    pendentes = [i for i in itens if not i["ok"]]
    return {"pode": not pendentes, "pendentes": len(pendentes), "itens": itens}


def texto(resultado: dict[str, Any]) -> str:
    cabeca = (
        "✅ Pode desligar."
        if resultado["pode"]
        else f"⚠️ Ainda não: {resultado['pendentes']} pendência(s)."
    )
    linhas = [f"{'✅' if i['ok'] else '⚠️'} {i['texto']}" for i in resultado["itens"]]
    return "\n".join([cabeca, *linhas])
