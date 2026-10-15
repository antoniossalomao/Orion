"""Seu estilo de código como skill (E7.8).

`orion estilo <pasta-repo> [...]` pede à CLI da sua assinatura (Claude Code, pelo `Delegator`) que
leia cada repositório e descreva o jeito de o Antônio escrever: nomes, comentários, testes e
commits. O resultado vira a skill `estilo-antonio`, mas **nasce desativada**: fica em
`<skills>/.pendentes/estilo-antonio/SKILL.md` (o catálogo ignora pastas com ponto) até você ler e
rodar `orion estilo --ativar`. O conteúdo é texto escrito por um modelo a partir de código: pode
errar e pode ecoar o que leu, por isso a revisão é sua e a skill continua sendo só texto (REGRAS.md:
skill é texto, não código).
"""

from __future__ import annotations

import re
import shutil
from pathlib import Path
from typing import Any

from .skills import MAX_CORPO, NOME

SKILL = "estilo-antonio"
DESCRICAO = (
    "O estilo de código do Antônio (nomes, comentários, testes, commits). Use ao escrever ou "
    "revisar código para ele."
)
PROMPT = (
    "Leia este repositório (código, testes e `git log`) e descreva, em português, o ESTILO de "
    "quem o escreve, para que outra pessoa o imite. Use exatamente estas seções em Markdown: "
    "## Nomes, ## Comentários, ## Testes, ## Commits, ## Estrutura. Seja concreto: padrões que "
    "se repetem, com 1 ou 2 exemplos curtos cada. Até 3000 caracteres. Não copie trechos longos "
    "de código, não inclua segredos, chaves, e-mails nem caminhos pessoais. Responda só com o "
    "Markdown, sem introdução."
)
_SECRETO = re.compile(
    r"(?i)(api[_-]?key|secret|token|password|senha)\s*[:=]\s*\S{8,}|-----BEGIN [A-Z ]*PRIVATE KEY"
)


class EstiloError(RuntimeError):
    """Não deu para gerar ou ativar a skill."""


def pendente(skills_dir: Path) -> Path:
    return skills_dir / ".pendentes" / SKILL / "SKILL.md"


def _limpar(texto: str) -> str:
    """O que a CLI devolveu, sem cercas de código externas e sem linha com cara de segredo."""
    t = texto.strip()
    t = re.sub(r"\A```(?:markdown|md)?\s*\n|\n```\s*\Z", "", t)
    return "\n".join(ln for ln in t.splitlines() if not _SECRETO.search(ln)).strip()


def montar(secoes: list[tuple[str, str]]) -> str:
    teto = max(500, (MAX_CORPO - 800) // max(1, len(secoes)))
    corpo = "\n\n".join(f"# Estilo em `{nome}`\n\n{texto[:teto]}" for nome, texto in secoes)
    return (
        f"---\nname: {SKILL}\ndescription: {DESCRICAO}\n---\n\n"
        "> Gerada por `orion estilo` (um modelo leu o código); revisada por você antes de valer. "
        " Não muda permissões: a política vale como sempre.\n\n" + corpo + "\n"
    )


def gerar(
    repos: list[Path], delegador: Any, skills_dir: Path, agente: str | None = "claude"
) -> Path:
    """Delega a leitura de cada repositório e grava a skill PENDENTE. Devolve o arquivo."""
    if not repos:
        raise EstiloError("informe ao menos uma pasta de repositório")
    secoes: list[tuple[str, str]] = []
    for repo in repos:
        raiz = repo.expanduser()
        if not raiz.is_dir():
            raise EstiloError(f"pasta inexistente: {repo}")
        r = delegador.delegate(PROMPT, str(raiz), agente)
        if not r.get("ok"):
            raise EstiloError(f"{raiz.name}: {r.get('erro', 'a CLI não respondeu')}")
        texto = _limpar(str(r.get("saida", "")))
        if len(texto) < 80:
            raise EstiloError(f"{raiz.name}: resposta curta demais para virar skill")
        secoes.append((raiz.name, texto))
    alvo = pendente(skills_dir)
    alvo.parent.mkdir(parents=True, exist_ok=True)
    alvo.write_text(montar(secoes), encoding="utf-8")
    return alvo


def ativar(skills_dir: Path) -> Path:
    """Move a skill pendente para o catálogo. Recusa trocar uma skill que já está valendo."""
    origem = pendente(skills_dir).parent
    if not (origem / "SKILL.md").is_file():
        raise EstiloError("não há skill pendente: rode `orion estilo <pasta-repo>` antes")
    if not NOME.fullmatch(SKILL):
        raise EstiloError("nome de skill inválido")
    destino = skills_dir / SKILL
    if destino.exists():
        raise EstiloError(f"{destino} já existe: apague-a (ou renomeie) para ativar a nova")
    shutil.move(str(origem), str(destino))
    return destino / "SKILL.md"
