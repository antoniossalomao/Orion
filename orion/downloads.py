"""Organizador de Downloads com regras suas (E9.4, REGRAS.md: automação sua vale pelo arquivo
concedido).

`<dados>/regras_downloads.yaml`:

    pasta: ~/Downloads              # opcional (padrão: a pasta Downloads do usuário)
    regras:
      - {extensoes: [pdf], padrao: "fatura*", destino: ~/Documents/Faturas}
      - {extensoes: [jpg, png], destino: ~/Pictures/Baixadas}

Só roda depois de `orion downloads conceder` (SHA-256 do arquivo, REGRAS.md: automação sua vale pelo
arquivo concedido). Move **só arquivos
soltos** da pasta (nada de subpasta, link, arquivo em download ou parado há menos de 30 s), nunca
sobrescreve (o repetido ganha ` (2)`), só para destinos que a política de escrita aceitaria sem
perguntar (Documents, Downloads, Desktop; nunca sistema, núcleo do Orion ou extensão sensível) e
registra cada movimento para `orion downloads desfazer`. O que não casa fica onde está.
"""

from __future__ import annotations

import fnmatch
import json
import logging
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

from .automacao import Concessoes, hash_do_arquivo

log = logging.getLogger("orion.downloads")

TIPO = "downloads"
NOME = "regras"
MAX_ARQUIVO = 16 * 1024
PARCIAIS = (".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload")
PARADO_S = 30.0
LOG_KEY = "downloads:log"
MAX_LOG = 500


class Regra(BaseModel):
    model_config = ConfigDict(extra="forbid")
    extensoes: list[str] = Field(default_factory=list, max_length=20)
    padrao: str | None = Field(default=None, max_length=100)
    destino: str = Field(min_length=1, max_length=400)

    @field_validator("extensoes")
    @classmethod
    def _ext(cls, v: list[str]) -> list[str]:
        return [e.lower().lstrip(".") for e in v if e.strip()]

    @field_validator("padrao")
    @classmethod
    def _padrao(cls, v: str | None) -> str | None:
        if v is not None and ("/" in v or "\\" in v):
            raise ValueError("o padrão é só de nome de arquivo, sem caminho")
        return v

    def casa(self, nome: str) -> bool:
        if not self.extensoes and not self.padrao:
            return False  # regra sem condição não casa com tudo por engano
        ext = nome.rsplit(".", 1)[-1].lower() if "." in nome else ""
        if self.extensoes and ext not in self.extensoes:
            return False
        return not self.padrao or fnmatch.fnmatch(nome.lower(), self.padrao.lower())


class Regras(BaseModel):
    model_config = ConfigDict(extra="forbid")
    pasta: str | None = Field(default=None, max_length=400)
    regras: list[Regra] = Field(min_length=1, max_length=50)


@dataclass
class Relatorio:
    movidos: list[dict[str, str]] = field(default_factory=list)
    ficaram: int = 0
    recusados: list[str] = field(default_factory=list)
    status: str = "ok"  # ok | nao_concedida | sem_regras | erro


class Downloads:
    def __init__(
        self,
        arquivo: Path,
        memory: Any,
        guard: Any,
        *,
        pasta_padrao: Path | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.arquivo, self.memory, self.guard = arquivo, memory, guard
        self.concessoes = Concessoes(memory)
        self._padrao = pasta_padrao or Path.home() / "Downloads"
        self._clock = clock

    # ── arquivo e concessão ───────────────────────────────────────────────────
    def carregar(self) -> tuple[Regras | None, str, str]:
        """(regras, hash, erro). Sem arquivo: (None, "", "")."""
        if not self.arquivo.is_file():
            return None, "", ""
        try:
            if self.arquivo.is_symlink() or self.arquivo.stat().st_size > MAX_ARQUIVO:
                raise ValueError("arquivo grande demais ou link")
            h = hash_do_arquivo(self.arquivo)
            bruto = yaml.safe_load(self.arquivo.read_text(encoding="utf-8"))
            if not isinstance(bruto, dict):
                raise ValueError("o arquivo precisa ser um mapa YAML")
            return Regras.model_validate(bruto), h, ""
        except ValidationError as e:
            msg = "; ".join(
                f"{'.'.join(str(x) for x in d['loc'])}: {d['msg']}" for d in e.errors()[:3]
            )
            return None, "", msg[:300]
        except (OSError, ValueError, yaml.YAMLError) as e:
            return None, "", str(e).splitlines()[0][:200]

    def estado(self) -> dict[str, Any]:
        regras, h, erro = self.carregar()
        return {
            "arquivo": str(self.arquivo),
            "existe": self.arquivo.is_file(),
            "erro": erro or None,
            "estado": self.concessoes.estado(TIPO, NOME, h) if regras else "sem_concessao",
            "regras": len(regras.regras) if regras else 0,
            "pasta": str(self._pasta(regras)) if regras else str(self._padrao),
        }

    def conceder(self) -> dict[str, Any]:
        regras, h, erro = self.carregar()
        if regras is None:
            raise ValueError(erro or "não há regras_downloads.yaml")
        self.concessoes.conceder(TIPO, NOME, h)
        return self.estado()

    def revogar(self) -> bool:
        return self.concessoes.revogar(TIPO, NOME)

    def _pasta(self, regras: Regras | None) -> Path:
        return Path(regras.pasta).expanduser() if regras and regras.pasta else self._padrao

    # ── movimentos ────────────────────────────────────────────────────────────
    def rodar(self, simular: bool = False) -> Relatorio:
        regras, h, _ = self.carregar()
        rel = Relatorio()
        if regras is None:
            rel.status = "sem_regras"
            return rel
        if self.concessoes.estado(TIPO, NOME, h) != "concedida":
            rel.status = "nao_concedida"
            return rel
        pasta = self._pasta(regras)
        if not pasta.is_dir():
            rel.status = "erro"
            return rel
        agora = self._clock()
        for f in sorted(pasta.iterdir()):
            if f.is_symlink() or not f.is_file() or f.name.startswith("."):
                continue
            if f.name.lower().endswith(PARCIAIS) or agora - f.stat().st_mtime < PARADO_S:
                rel.ficaram += 1
                continue
            regra = next((r for r in regras.regras if r.casa(f.name)), None)
            if regra is None:
                rel.ficaram += 1
                continue
            destino_dir = Path(regra.destino).expanduser()
            motivo = self.guard.check_write(str(destino_dir / f.name))
            if motivo:
                rel.recusados.append(f"{f.name}: {motivo}")
                continue
            alvo = _livre(destino_dir / f.name)
            if not simular:
                destino_dir.mkdir(parents=True, exist_ok=True)
                _mover(f, alvo)
                self._registrar(f, alvo)
            rel.movidos.append({"de": str(f), "para": str(alvo)})
        return rel

    # ── registro e desfazer ───────────────────────────────────────────────────
    def _log(self) -> list[dict[str, Any]]:
        try:
            dado = json.loads(self.memory.meta_get(LOG_KEY) or "[]")
            return dado if isinstance(dado, list) else []
        except ValueError:
            return []

    def _registrar(self, de: Path, para: Path) -> None:
        lista = self._log()
        lista.append({"ts": self._clock(), "de": str(de), "para": str(para), "desfeito": False})
        self.memory.meta_set(LOG_KEY, json.dumps(lista[-MAX_LOG:], ensure_ascii=False))

    def historico(self, limite: int = 50) -> list[dict[str, Any]]:
        return list(reversed(self._log()))[: max(1, min(limite, MAX_LOG))]

    def desfazer(self, escopo: str = "ultimo") -> dict[str, Any]:
        """`ultimo` desfaz o último movimento; `hoje` desfaz todos de hoje (do mais novo ao mais
        velho). Só volta o que ainda está onde foi posto e cujo lugar de origem está livre."""
        if escopo not in ("ultimo", "hoje"):
            raise ValueError("escopo: ultimo ou hoje")
        lista = self._log()
        inicio_dia = (
            datetime.fromtimestamp(self._clock())
            .replace(hour=0, minute=0, second=0, microsecond=0)
            .timestamp()
        )
        candidatos = [
            i
            for i, m in reversed(list(enumerate(lista)))
            if not m.get("desfeito") and (escopo == "ultimo" or m["ts"] >= inicio_dia)
        ]
        if escopo == "ultimo":
            candidatos = candidatos[:1]
        desfeitos, pulados = [], []
        for i in candidatos:
            m = lista[i]
            origem, atual = Path(m["de"]), Path(m["para"])
            if not atual.is_file():
                pulados.append(f"{atual.name}: não está mais onde foi posto")
            elif origem.exists():
                pulados.append(f"{origem.name}: o lugar de origem já está ocupado")
            else:
                _mover(atual, origem)
                lista[i]["desfeito"] = True
                desfeitos.append(m["de"])
        self.memory.meta_set(LOG_KEY, json.dumps(lista, ensure_ascii=False))
        return {"desfeitos": desfeitos, "pulados": pulados}


def _livre(alvo: Path) -> Path:
    """Nunca sobrescreve: `nome.ext`, `nome (2).ext`, `nome (3).ext`..."""
    if not alvo.exists():
        return alvo
    for n in range(2, 1000):
        candidato = alvo.with_name(f"{alvo.stem} ({n}){alvo.suffix}")
        if not candidato.exists():
            return candidato
    raise OSError("sem nome livre no destino")


def _mover(de: Path, para: Path) -> None:
    import shutil

    para.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(de), str(para))


EXEMPLO = """\
# Regras de Downloads. Para valer: orion downloads conceder
pasta: ~/Downloads
regras:
  - {extensoes: [pdf], padrao: "fatura*", destino: ~/Documents/Faturas}
  - {extensoes: [jpg, jpeg, png, webp], destino: ~/Documents/Imagens baixadas}
"""
