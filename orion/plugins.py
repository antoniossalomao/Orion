"""Plugins locais (C19–C30, regra 45): pacote de skills e servidores MCP, instalado e concedido.

Um plugin é uma pasta com `plugin.json` (nome, versão, descrição), uma pasta opcional `skills/`
(cada skill é uma pasta com `SKILL.md`, ver `orion.skills`) e servidores MCP declarados no próprio
manifesto (no formato do `mcp.json`, com a classe de risco de cada ferramenta).

- **Instalar não executa nada**: copiar a pasta para `<dados>/plugins/<nome>` só valida e copia
  (sem link simbólico, até 200 arquivos e 5 MB). Nenhum comando do manifesto roda na instalação.
- **Nada vale sem concessão**: depois de instalado o plugin fica inativo. `conceder` registra o
  hash do pacote inteiro (manifesto + todos os arquivos); `revogar` tira. **Qualquer mudança no
  pacote (atualização, edição) invalida a concessão** até você conceder de novo.
- **O que a concessão libera** é exatamente o que `describe()` mostra: as skills (texto, regra 40)
  e os servidores MCP (que sobem sob a política de risco de sempre, regra 24; a classe vem do
  manifesto que você leu, nunca do servidor). Mudanças valem depois de reiniciar o Orion.
"""

from __future__ import annotations

import hashlib
import json
import re
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

from .mcp_client import ServerConfig

NOME = re.compile(r"^[a-z][a-z0-9]{0,11}$")  # curto: vira prefixo do nome do servidor MCP
NOME_SERVIDOR = re.compile(r"^[a-z][a-z0-9]{0,10}$")
MAX_ARQUIVOS = 200
MAX_BYTES = 5 * 1024 * 1024
ESTADO = ".estado.json"


class PluginError(ValueError):
    """Pacote inválido ou operação recusada; a mensagem é segura de mostrar."""


class Manifesto(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str
    version: str = Field(min_length=1, max_length=32)
    description: str = Field(min_length=1, max_length=300)
    skills: str | None = (
        None  # pasta (relativa ao plugin) com as skills; padrão: `skills/` se existir
    )
    mcp: dict[str, ServerConfig] = Field(default_factory=dict)

    @field_validator("name")
    @classmethod
    def _nome(cls, v: str) -> str:
        if not NOME.match(v):
            raise ValueError("name: minúsculas e dígitos, começando por letra, até 12")
        return v

    @field_validator("mcp")
    @classmethod
    def _servidores(cls, v: dict[str, ServerConfig]) -> dict[str, ServerConfig]:
        for n in v:
            if not NOME_SERVIDOR.match(n):
                raise ValueError(f"mcp.{n}: nome de servidor inválido (minúsculas/dígitos, até 11)")
        return v


@dataclass(frozen=True)
class Plugin:
    nome: str
    versao: str
    descricao: str
    pasta: Path
    manifesto: Manifesto
    hash: str
    concedido: bool  # há concessão e ela ainda vale para ESTE pacote
    mudou: bool  # já houve concessão, mas o pacote mudou depois (precisa conceder de novo)
    skills: int
    servidores: dict[str, ServerConfig]


def _arquivos(pasta: Path) -> list[Path]:
    return sorted(p for p in pasta.rglob("*") if p.is_file() and p.name != ESTADO)


def hash_do_pacote(pasta: Path) -> str:
    h = hashlib.sha256()
    for p in _arquivos(pasta):
        h.update(p.relative_to(pasta).as_posix().encode() + b"\0")
        h.update(hashlib.sha256(p.read_bytes()).digest())
    return h.hexdigest()


def _validar_pasta(pasta: Path) -> None:
    if not pasta.is_dir():
        raise PluginError("a pasta do plugin não existe")
    n, total = 0, 0
    for p in pasta.rglob("*"):
        if p.is_symlink():
            raise PluginError(f"link simbólico não é aceito: {p.relative_to(pasta).as_posix()}")
        if p.is_file():
            n += 1
            total += p.stat().st_size
    if n > MAX_ARQUIVOS or total > MAX_BYTES:
        raise PluginError(f"pacote grande demais (máx. {MAX_ARQUIVOS} arquivos e 5 MB)")


def ler_manifesto(pasta: Path) -> Manifesto:
    arquivo = pasta / "plugin.json"
    if not arquivo.is_file():
        raise PluginError("falta o plugin.json")
    try:
        return Manifesto.model_validate(json.loads(arquivo.read_text(encoding="utf-8")))
    except (ValueError, ValidationError) as e:
        raise PluginError(f"plugin.json inválido: {str(e)[:300]}") from None


class PluginStore:
    def __init__(self, raiz: Path | str) -> None:
        self.raiz = Path(raiz)

    # ── estado das concessões ─────────────────────────────────────────────
    def _estado(self) -> dict[str, str]:
        try:
            dado = json.loads((self.raiz / ESTADO).read_text(encoding="utf-8"))
            return {str(k): str(v) for k, v in dado.items()} if isinstance(dado, dict) else {}
        except (OSError, ValueError):
            return {}

    def _gravar_estado(self, estado: dict[str, str]) -> None:
        self.raiz.mkdir(parents=True, exist_ok=True)
        (self.raiz / ESTADO).write_text(json.dumps(estado, indent=1), encoding="utf-8")

    # ── leitura ───────────────────────────────────────────────────────────
    def lista(self) -> list[Plugin]:
        if not self.raiz.is_dir():
            return []
        estado = self._estado()
        saida: list[Plugin] = []
        for pasta in sorted(p for p in self.raiz.iterdir() if p.is_dir()):
            try:
                _validar_pasta(pasta)
                m = ler_manifesto(pasta)
                if m.name != pasta.name:
                    continue
                h = hash_do_pacote(pasta)
            except (PluginError, OSError):
                continue  # pasta estranha: ignorada (não derruba os outros)
            conc = estado.get(m.name)
            skills_dir = pasta / (m.skills or "skills")
            n_skills = (
                sum((d / "SKILL.md").is_file() for d in skills_dir.iterdir() if d.is_dir())
                if skills_dir.is_dir()
                else 0
            )
            saida.append(
                Plugin(
                    m.name,
                    m.version,
                    m.description,
                    pasta,
                    m,
                    h,
                    concedido=conc == h,
                    mudou=conc is not None and conc != h,
                    skills=n_skills,
                    servidores=m.mcp,
                )
            )
        return saida

    def get(self, nome: str) -> Plugin | None:
        return next((p for p in self.lista() if p.nome == nome), None)

    def ativos(self) -> list[Plugin]:
        return [p for p in self.lista() if p.concedido]

    def pastas_de_skills(self) -> list[Path]:
        return [
            p.pasta / (p.manifesto.skills or "skills")
            for p in self.ativos()
            if (p.pasta / (p.manifesto.skills or "skills")).is_dir()
        ]

    def servidores_mcp(self) -> dict[str, ServerConfig]:
        """Servidores dos plugins concedidos, com o nome `<plugin><servidor>` (único, sem `_`)."""
        saida: dict[str, ServerConfig] = {}
        for p in self.ativos():
            for nome, cfg in p.servidores.items():
                chave = f"{p.nome}{nome}"
                if chave not in saida:  # colisão (ab+cd × abc+d): vale o primeiro
                    saida[chave] = cfg
        return saida

    # ── escrita ───────────────────────────────────────────────────────────
    def instalar(self, origem: Path | str, *, atualizar: bool = False) -> Plugin:
        """Valida e copia. NÃO concede nada e NÃO executa nada do pacote."""
        origem = Path(origem).expanduser().resolve()
        _validar_pasta(origem)
        m = ler_manifesto(origem)
        destino = self.raiz / m.name
        if destino.exists() and not atualizar:
            raise PluginError(f"o plugin '{m.name}' já está instalado (use atualizar)")
        self.raiz.mkdir(parents=True, exist_ok=True)
        tmp = self.raiz / f".instalando-{m.name}"
        shutil.rmtree(tmp, ignore_errors=True)
        shutil.copytree(origem, tmp, symlinks=False)
        if destino.exists():
            shutil.rmtree(destino)
        tmp.rename(destino)
        plugin = self.get(m.name)
        assert plugin is not None
        return plugin

    def conceder(self, nome: str) -> Plugin:
        p = self.get(nome)
        if p is None:
            raise PluginError(f"plugin '{nome}' não encontrado")
        estado = self._estado()
        estado[nome] = p.hash
        self._gravar_estado(estado)
        concedido = self.get(nome)
        assert concedido is not None
        return concedido

    def revogar(self, nome: str) -> bool:
        estado = self._estado()
        if nome not in estado:
            return False
        del estado[nome]
        self._gravar_estado(estado)
        return True

    def remover(self, nome: str) -> bool:
        if not NOME.match(nome) or not (self.raiz / nome).is_dir():
            return False
        self.revogar(nome)
        shutil.rmtree(self.raiz / nome)
        return True


def describe(p: Plugin) -> dict[str, Any]:
    """O que a concessão libera, para a pessoa ler antes de conceder (CLI, API e interface)."""
    servidores = []
    for nome, cfg in p.servidores.items():
        servidores.append(
            {
                "nome": f"{p.nome}{nome}",
                "transporte": "remoto (HTTP)" if cfg.url else "local (comando)",
                "executa": cfg.url or " ".join([cfg.command or "", *cfg.args])[:200],
                "risco_padrao": cfg.default_risk.value,
                "ferramentas": {
                    t: r.risk.value if r.risk else cfg.default_risk.value
                    for t, r in cfg.tools.items()
                },
                "externo": cfg.external,
            }
        )
    return {
        "nome": p.nome,
        "versao": p.versao,
        "descricao": p.descricao,
        "skills": p.skills,
        "servidores": servidores,
        "estado": "ativo" if p.concedido else "mudou" if p.mudou else "sem_concessao",
    }
