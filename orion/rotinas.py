"""Rotinas (E9.1, regra 58): uma sequência de passos que **você** escreveu e concedeu.

`<dados>/rotinas/<nome>.yaml`:

    descricao: Bom dia
    gatilhos: [{tipo: frase, valor: bom dia orion}, {tipo: palmas}, {tipo: telegram}]
    liberar: [abrir_app]            # opcional: o que a concessão libera a rodar sem perguntar
    passos:
      - briefing: true              # o briefing do dia vira um aviso
      - ferramenta: controlar_midia
        argumentos: {acao: tocar_pausar}
      - falar: "Tudo pronto."

Quem decide o que roda é o arquivo, não o modelo. Passo de leitura roda direto; `controlar_midia` e,
se a rotina listar em `liberar`, `abrir_app` rodam direto com os argumentos fixos do arquivo; tudo o
mais vira o pedido de aprovação de sempre (botão) **e a rotina para ali**. Cada passo vai ao audit.
"""

from __future__ import annotations

import asyncio
import json
import logging
import re
import unicodedata
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, model_validator

from .automacao import Concessoes, hash_do_arquivo
from .policy.classes import Risk

log = logging.getLogger("orion.rotinas")

NOME = re.compile(r"^[a-z0-9][a-z0-9_-]{0,31}$")
MAX_ARQUIVO = 16 * 1024
MAX_PASSOS = 20
DIRETAS = frozenset({"controlar_midia"})  # rodam direto em qualquer rotina concedida
LIBERAVEIS = frozenset({"abrir_app"})  # só se a rotina listar em `liberar`
TIPO = "rotina"
GATILHOS_AUTOMATICOS = frozenset({"palmas", "tecla"})  # o não perturbe segura estes


def normalizar_frase(texto: str) -> str:
    sem = unicodedata.normalize("NFKD", texto.casefold())
    sem = "".join(c for c in sem if not unicodedata.combining(c))
    return " ".join(re.sub(r"[^a-z0-9 ]+", " ", sem).split())


class Gatilho(BaseModel):
    model_config = ConfigDict(extra="forbid")
    tipo: Literal["frase", "palmas", "tecla", "telegram"]
    valor: str | None = Field(default=None, max_length=80)

    @model_validator(mode="after")
    def _frase_tem_valor(self) -> Gatilho:
        if self.tipo == "frase" and not (self.valor and normalizar_frase(self.valor)):
            raise ValueError("gatilho 'frase' precisa de um valor")
        return self


class Passo(BaseModel):
    model_config = ConfigDict(extra="forbid")
    ferramenta: str | None = Field(default=None, pattern=r"^[a-z][a-z0-9_]{0,63}$")
    argumentos: dict[str, Any] = Field(default_factory=dict)
    falar: str | None = Field(default=None, max_length=500)
    briefing: bool | None = None

    @model_validator(mode="after")
    def _um_tipo_so(self) -> Passo:
        tipos = [self.ferramenta is not None, self.falar is not None, bool(self.briefing)]
        if sum(tipos) != 1:
            raise ValueError("cada passo tem exatamente um de: ferramenta, falar, briefing")
        if self.argumentos and self.ferramenta is None:
            raise ValueError("'argumentos' só vale com 'ferramenta'")
        return self


class Rotina(BaseModel):
    model_config = ConfigDict(extra="forbid")
    descricao: str = Field(default="", max_length=200)
    gatilhos: list[Gatilho] = Field(default_factory=list, max_length=10)
    liberar: list[str] = Field(default_factory=list, max_length=5)
    passos: list[Passo] = Field(min_length=1, max_length=MAX_PASSOS)

    @model_validator(mode="after")
    def _liberar_conhecido(self) -> Rotina:
        ruim = [x for x in self.liberar if x not in LIBERAVEIS]
        if ruim:
            raise ValueError(
                f"'liberar' só aceita: {', '.join(sorted(LIBERAVEIS))} (não {ruim[0]})"
            )
        return self


@dataclass(frozen=True)
class Carregada:
    nome: str
    arquivo: Path
    hash: str
    rotina: Rotina | None
    erro: str | None
    estado: str  # concedida | mudou | sem_concessao


@dataclass
class Resultado:
    nome: str
    status: str  # concluida | aguardando_aprovacao | recusada | falhou | nao_concedida
    passos: list[dict[str, Any]] = field(default_factory=list)
    aprovacao_id: str | None = None
    motivo: str = ""

    def publico(self) -> dict[str, Any]:
        return {
            "nome": self.nome,
            "status": self.status,
            "passos": self.passos,
            "aprovacao_id": self.aprovacao_id,
            "motivo": self.motivo,
        }


class RotinaStore:
    def __init__(self, pasta: Path, memory: Any) -> None:
        self.pasta = pasta
        self.concessoes = Concessoes(memory)

    def _ler(self, arquivo: Path) -> Carregada:
        nome = arquivo.stem
        try:
            if arquivo.is_symlink() or arquivo.stat().st_size > MAX_ARQUIVO:
                raise ValueError("arquivo grande demais ou link")
            hash_ = hash_do_arquivo(arquivo)
            bruto = yaml.safe_load(arquivo.read_text(encoding="utf-8"))
            if not isinstance(bruto, dict):
                raise ValueError("o arquivo precisa ser um mapa YAML")
            rotina = Rotina.model_validate(bruto)
        except ValidationError as e:
            erro = "; ".join(
                f"{'.'.join(str(x) for x in d['loc']) or 'arquivo'}: {d['msg']}"
                for d in e.errors()[:3]
            )
            return Carregada(nome, arquivo, "", None, erro[:300], "sem_concessao")
        except (OSError, ValueError, yaml.YAMLError) as e:
            return Carregada(nome, arquivo, "", None, str(e).splitlines()[0][:200], "sem_concessao")
        return Carregada(
            nome, arquivo, hash_, rotina, None, self.concessoes.estado(TIPO, nome, hash_)
        )

    def listar(self) -> list[Carregada]:
        if not self.pasta.is_dir():
            return []
        return [
            self._ler(a)
            for a in sorted(self.pasta.glob("*.yaml"))
            if NOME.match(a.stem) and a.is_file()
        ]

    def obter(self, nome: str) -> Carregada | None:
        if not NOME.match(nome):
            return None
        arquivo = self.pasta / f"{nome}.yaml"
        return self._ler(arquivo) if arquivo.is_file() else None

    def conceder(self, nome: str) -> Carregada:
        c = self.obter(nome)
        if c is None:
            raise KeyError(nome)
        if c.rotina is None:
            raise ValueError(f"a rotina '{nome}' tem erro: {c.erro}")
        self.concessoes.conceder(TIPO, nome, c.hash)
        return self._ler(c.arquivo)

    def revogar(self, nome: str) -> bool:
        return self.concessoes.revogar(TIPO, nome)

    def por_frase(self, texto: str) -> Carregada | None:
        """A rotina **concedida** cuja frase bate exatamente (sem acento, caixa nem pontuação)."""
        alvo = normalizar_frase(texto)
        if not alvo:
            return None
        for c in self.listar():
            if c.rotina is None or c.estado != "concedida":
                continue
            if any(
                g.tipo == "frase" and normalizar_frase(g.valor or "") == alvo
                for g in c.rotina.gatilhos
            ):
                return c
        return None


class Executor:
    """Roda uma rotina concedida. `tools`, `policy` e `memory` são os do app."""

    def __init__(
        self,
        *,
        store: RotinaStore,
        tools: Any,
        policy: Any,
        memory: Any,
        ops: Any,
        briefing: Callable[[], str] | None = None,
        modos: Any = None,
        passo_timeout_s: float = 60.0,
    ) -> None:
        self.store, self.tools, self.policy, self.memory, self.ops = (
            store,
            tools,
            policy,
            memory,
            ops,
        )
        self._briefing, self._modos, self._timeout = briefing, modos, passo_timeout_s
        self._rodando: set[str] = set()

    def _bloqueio(self, origem: str) -> str:
        if self._modos is None:
            return ""
        if self._modos.panico():
            return "modo pânico"
        if origem in GATILHOS_AUTOMATICOS and self._modos.nao_perturbe():
            return "não perturbe"
        return ""

    def _audit(self, nome: str, passo: Passo, acao: str, risco: str | None, motivo: str) -> None:
        self.ops.audit_add(
            {
                "tool": f"rotina_{passo.ferramenta or 'texto'}",
                "action": acao,
                "risk": risco,
                "reason": f"rotina={nome}; {motivo}"[:200],
                "args": {k: type(v).__name__ for k, v in passo.argumentos.items()},
            }
        )

    async def executar(self, nome: str, origem: str = "manual") -> Resultado:
        c = self.store.obter(nome)
        if c is None or c.rotina is None:
            return Resultado(nome, "falhou", motivo=(c.erro if c else None) or "rotina não existe")
        if c.estado != "concedida":
            motivo = (
                "o arquivo mudou depois da concessão" if c.estado == "mudou" else "sem concessão"
            )
            return Resultado(
                nome, "nao_concedida", motivo=f"{motivo}: rode `orion rotina conceder {nome}`"
            )
        exige = {"frase": {"frase"}, "telegram": {"telegram", "frase"}}.get(origem)
        if exige and not any(g.tipo in exige for g in c.rotina.gatilhos):
            return Resultado(nome, "recusada", motivo=f"a rotina não tem gatilho '{origem}'")
        bloqueado = self._bloqueio(origem)
        if bloqueado:
            return Resultado(nome, "recusada", motivo=bloqueado)
        if nome in self._rodando:
            return Resultado(nome, "recusada", motivo="essa rotina já está rodando")
        self._rodando.add(nome)
        try:
            return await self._rodar(c, origem)
        finally:
            self._rodando.discard(nome)

    async def _rodar(self, c: Carregada, origem: str) -> Resultado:

        rotina, nome = c.rotina, c.nome
        assert rotina is not None
        res = Resultado(nome, "concluida")
        sessao = None  # só nasce se um passo precisar de aprovação
        for i, passo in enumerate(rotina.passos, 1):
            if self.store.concessoes.estado(TIPO, nome, c.hash) != "concedida":
                res.status, res.motivo = "recusada", "a concessão caiu durante a rotina"
                break
            info: dict[str, Any] = {
                "n": i,
                "tipo": "falar"
                if passo.falar
                else "briefing"
                if passo.briefing
                else passo.ferramenta,
            }
            try:
                if passo.falar is not None:
                    self._audit(nome, passo, "allow", "read", f"origem={origem}; falar")
                    self.ops.notify("rotina", passo.falar, urgente=True)
                    info["estado"] = "ok"
                elif passo.briefing:
                    self._audit(nome, passo, "allow", "read", f"origem={origem}; briefing")
                    texto = self._briefing() if self._briefing else "(briefing indisponível)"
                    self.ops.notify("rotina", texto, urgente=True)
                    info["estado"] = "ok"
                else:
                    assert passo.ferramenta is not None
                    spec = self.policy.tools.get(passo.ferramenta)
                    tool = self.tools.get(passo.ferramenta)
                    if spec is None or tool is None:
                        res.status, res.motivo = (
                            "falhou",
                            f"ferramenta '{passo.ferramenta}' não existe aqui",
                        )
                        info["estado"] = "falhou"
                        res.passos.append(info)
                        break
                    direto = (
                        (spec.risk is Risk.READ and not spec.egress and not spec.external)
                        or passo.ferramenta in DIRETAS
                        or (passo.ferramenta in LIBERAVEIS and passo.ferramenta in rotina.liberar)
                    )
                    if not direto:
                        if sessao is None:
                            sessao = self.memory.new_session(
                                "telegram" if origem == "telegram" else "web", f"Rotina: {nome}"
                            )
                        a = self.policy.approvals.request(
                            sessao.id,
                            passo.ferramenta,
                            passo.argumentos,
                            f"passo {i} da rotina {nome}",
                        )
                        self._audit(
                            nome, passo, "confirm", spec.risk.value, f"origem={origem}; passo {i}"
                        )
                        res.status, res.aprovacao_id = "aguardando_aprovacao", a.id
                        res.motivo = f"o passo {i} ({passo.ferramenta}) espera a sua aprovação"
                        info["estado"] = "aguardando_aprovacao"
                        res.passos.append(info)
                        break
                    self._audit(nome, passo, "allow", spec.risk.value, f"origem={origem}; direto")
                    bruto = await asyncio.wait_for(
                        tool.run_async(passo.argumentos), timeout=self._timeout
                    )
                    dados = _json(bruto)
                    if isinstance(dados, dict) and (dados.get("ok") is False or dados.get("erro")):
                        info["estado"] = "falhou"
                        detalhe = str(dados.get("erro") or "falhou")[:160]
                        res.status, res.motivo = "falhou", f"passo {i}: {detalhe}"
                        res.passos.append(info)
                        break
                    info["estado"] = "ok"
            except TimeoutError:
                info["estado"] = "falhou"
                res.status, res.motivo = "falhou", f"passo {i}: tempo esgotado"
                res.passos.append(info)
                break
            res.passos.append(info)
        self.ops.audit_add(
            {
                "tool": "rotina",
                "action": "allow",
                "risk": "read",
                "reason": f"{nome}: {res.status}; origem={origem}"[:200],
            }
        )
        return res


def _json(bruto: str) -> Any:
    try:
        return json.loads(bruto)
    except ValueError:
        return None


EXEMPLO_BOM_DIA = """\
# Rotina de exemplo. Para valer: orion rotina conceder bom-dia
descricao: Bom dia — o briefing do dia, música e abrir o Orion
gatilhos:
  - {tipo: frase, valor: bom dia orion}
  - {tipo: telegram}
liberar: [abrir_app]   # sem isto o passo abrir_app pediria a sua aprovação
passos:
  - briefing: true
  - ferramenta: controlar_midia
    argumentos: {acao: tocar_pausar}
  - ferramenta: abrir_app
    argumentos: {nome: "http://127.0.0.1:8000/ui/"}   # Windows/macOS abrem a página; ajuste a porta
  - falar: "Bom dia! O briefing está nos avisos."
"""
