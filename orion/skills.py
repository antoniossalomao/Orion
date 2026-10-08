"""Skills no formato aberto Agent Skills (`<pasta>/<nome>/SKILL.md`), com carregamento gradual.

Só o nome e a descrição entram no prompt de todo turno; o corpo vem sob demanda pela
ferramenta `carregar_skill`. Uma skill é TEXTO que orienta o modelo: nada aqui executa
script, e a pasta `scripts/` de um pacote é ignorada (e avisada). `references/` só tem os
nomes listados. A skill não muda a política: toda ferramenta que ela mandar usar continua
passando pelas mesmas classes de risco e aprovações.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass, field
from pathlib import Path

from .policy.classes import Risk, ToolSpec
from .tools.registry import Tool

log = logging.getLogger("orion.skills")

NOME = re.compile(r"[a-z0-9][a-z0-9-]{0,63}")
MAX_DESCRICAO = 1024
MAX_CORPO = 20_000
MAX_NO_PROMPT = 30
_FRONT = re.compile(r"\A---\s*\n(.*?)\n---\s*(?:\n|\Z)", re.DOTALL)

TOOL_SPEC = ToolSpec("carregar_skill", Risk.READ)


@dataclass(frozen=True)
class Skill:
    nome: str
    descricao: str
    caminho: Path
    avisos: tuple[str, ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class Rejeitada:
    pasta: str
    motivo: str


def _valor(bruto: str) -> str:
    v = bruto.strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        v = v[1:-1]
    return v


def _frontmatter(texto: str) -> tuple[dict[str, str], str]:
    """YAML mínimo (`chave: valor`, uma linha): basta para nome e descrição, sem dependência."""
    m = _FRONT.match(texto)
    if not m:
        return {}, texto
    campos: dict[str, str] = {}
    for linha in m.group(1).splitlines():
        if ":" in linha and not linha.startswith((" ", "\t", "#")):
            k, _, v = linha.partition(":")
            campos[k.strip().lower()] = _valor(v)
    return campos, texto[m.end() :]


class SkillCatalog:
    def __init__(self, raiz: Path | str, extras: list[Path] | None = None) -> None:
        """`extras`: pastas de skills de plugins concedidos (a sua própria pasta vale primeiro)."""
        self.raiz = Path(raiz)
        self.extras = [Path(e) for e in extras or []]
        self.skills: dict[str, Skill] = {}
        self.rejeitadas: list[Rejeitada] = []
        self.scan()

    def scan(self) -> None:
        self.skills, self.rejeitadas = {}, []
        for raiz in (self.raiz, *self.extras):
            if not raiz.is_dir():
                continue
            base = raiz.resolve()
            for pasta in sorted(raiz.iterdir()):
                if not pasta.is_dir() or pasta.name.startswith("."):
                    continue
                try:
                    self._ler(pasta, base)
                except OSError as e:
                    self.rejeitadas.append(Rejeitada(pasta.name, f"ilegível: {type(e).__name__}"))

    def _ler(self, pasta: Path, base: Path) -> None:
        arquivo = pasta / "SKILL.md"
        if pasta.is_symlink() or arquivo.is_symlink() or not arquivo.is_file():
            self.rejeitadas.append(Rejeitada(pasta.name, "sem SKILL.md regular (link não vale)"))
            return
        if not arquivo.resolve().is_relative_to(base):
            self.rejeitadas.append(Rejeitada(pasta.name, "SKILL.md fora da pasta de skills"))
            return
        if arquivo.stat().st_size > MAX_CORPO * 4:
            self.rejeitadas.append(Rejeitada(pasta.name, "SKILL.md grande demais"))
            return
        campos, corpo = _frontmatter(arquivo.read_text(encoding="utf-8", errors="replace"))
        nome, desc = campos.get("name", ""), campos.get("description", "")
        if not NOME.fullmatch(nome):
            self.rejeitadas.append(Rejeitada(pasta.name, "name inválido (a-z, 0-9 e hífen)"))
        elif nome != pasta.name:
            self.rejeitadas.append(Rejeitada(pasta.name, f"name '{nome}' difere da pasta"))
        elif not desc or len(desc) > MAX_DESCRICAO:
            self.rejeitadas.append(Rejeitada(pasta.name, "description vazia ou longa demais"))
        elif len(corpo) > MAX_CORPO:
            self.rejeitadas.append(Rejeitada(pasta.name, f"corpo acima de {MAX_CORPO} caracteres"))
        elif nome in self.skills:
            self.rejeitadas.append(Rejeitada(pasta.name, "nome já usado por outra skill"))
        else:
            avisos = (
                ("scripts/ ignorada: skill não executa código",)
                if (pasta / "scripts").exists()
                else ()
            )
            self.skills[nome] = Skill(nome, desc, arquivo, avisos)

    # ── uso ───────────────────────────────────────────────────────────────
    def prompt_block(self) -> str:
        """Só nome e descrição, para todo turno. Vazio se não há skill válida."""
        if not self.skills:
            return ""
        itens = list(self.skills.values())[:MAX_NO_PROMPT]
        linhas = "\n".join(f"- {s.nome}: {s.descricao}" for s in itens)
        return (
            "[SKILLS: instruções do Antônio, uma por assunto. Quando o pedido combinar com a "
            "descrição, chame `carregar_skill` com o nome antes de agir. Uma skill não dá "
            "permissão extra: as aprovações valem do mesmo jeito.]\n" + linhas
        )

    def load(self, nome: str) -> dict[str, object]:
        s = self.skills.get(nome.strip().lower())
        if s is None:
            return {"erro": f"skill '{nome}' não existe", "disponiveis": sorted(self.skills)}
        _, corpo = _frontmatter(s.caminho.read_text(encoding="utf-8", errors="replace"))
        refs = s.caminho.parent / "references"
        nomes = sorted(p.name for p in refs.iterdir() if p.is_file()) if refs.is_dir() else []
        out: dict[str, object] = {"nome": s.nome, "instrucoes": corpo.strip()[:MAX_CORPO]}
        if nomes:
            out["referencias_na_pasta"] = nomes[:50]
        if s.avisos:
            out["avisos"] = list(s.avisos)
        return out


def skill_tool(catalogo: SkillCatalog) -> Tool:
    return Tool(
        "carregar_skill",
        "Carrega as instruções completas de uma skill do Antônio pelo nome.",
        {
            "type": "object",
            "properties": {"nome": {"type": "string"}},
            "required": ["nome"],
        },
        lambda nome: catalogo.load(nome),
    )
