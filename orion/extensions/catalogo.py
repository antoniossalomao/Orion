"""Catálogo de plugins, local e remoto (E8.4, REGRAS.md: catálogo de plugins: baixar não instala,
nada atualiza sozinho).

- **Local**: a pasta `<dados>/catalogo/` com `catalogo.json` e os `.zip` que ele lista.
- **Remoto**: só as URLs que você cadastrou em `ORION_PLUGIN_CATALOGS`. Baixa-se o índice
  (até 256 KB) e, quando você manda instalar, **um** pacote do mesmo host do índice, sempre pela
  barreira de rede (`netguard`). O SHA-256 que o índice fixa é conferido antes de o pacote entrar
  em qualquer lugar.

Baixar não instala, instalar não concede (REGRAS.md: plugin é pacote sem poder próprio) e nada
atualiza sozinho: uma versão nova só
aparece como aviso. O índice e o pacote são texto/dados de terceiros e nunca executam.
"""

from __future__ import annotations

import hashlib
import json
import re
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urljoin, urlsplit

from pydantic import BaseModel, ConfigDict, Field, ValidationError

from .. import netguard, saidas
from .archives import MAX_ARCHIVE
from .plugins import _ID, _VERSION, PluginError

MAX_INDICE = 256 * 1024
MAX_ENTRADAS = 200
MAX_FONTES = 5
LOCAL = "local"
_ARQUIVO = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\.zip$")

Buscar = Callable[..., netguard.Resposta]


class EntradaJson(BaseModel):
    model_config = ConfigDict(extra="forbid")
    id: str = Field(pattern=_ID)
    name: str = Field(min_length=1, max_length=80)
    version: str = Field(pattern=_VERSION, max_length=80)
    description: str = Field(default="", max_length=1024)
    sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    file: str | None = Field(default=None, pattern=_ARQUIVO.pattern)  # local
    url: str | None = Field(default=None, max_length=2048)  # remoto: absoluta ou relativa ao índice


class IndiceJson(BaseModel):
    model_config = ConfigDict(extra="forbid")
    plugins: list[EntradaJson] = Field(max_length=MAX_ENTRADAS)


@dataclass(frozen=True)
class Entrada:
    fonte: str  # "local" ou a URL do índice
    id: str
    name: str
    version: str
    description: str
    sha256: str
    destino: str  # arquivo (local) ou URL do pacote (remoto)

    def publico(self) -> dict:
        return {
            "fonte": self.fonte,
            "id": self.id,
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "sha256": self.sha256,
        }


def versao_maior(a: str, b: str) -> bool:
    """a > b, por número (a parte depois do `-` conta como pré-lançamento: vale menos)."""

    def chave(v: str) -> tuple:
        base, _, pre = v.partition("-")
        return (*(int(x) for x in base.split(".")), 0 if pre else 1)

    return chave(a) > chave(b)


def urls_validas(bruto: str) -> list[str]:
    """`ORION_PLUGIN_CATALOGS`: URLs https separadas por `;`, até 5. Levanta ValueError."""
    urls = [u.strip() for u in bruto.split(";") if u.strip()]
    if len(urls) > MAX_FONTES:
        raise ValueError(f"no máximo {MAX_FONTES} catálogos")
    for u in urls:
        p = urlsplit(u)
        if p.scheme != "https" or not p.hostname or p.username or p.fragment:
            raise ValueError(f"catálogo inválido (use https, sem usuário): {u[:80]}")
    return urls


class Catalogos:
    def __init__(self, pasta: Path, urls: list[str] | None = None, buscar: Buscar | None = None):
        self.pasta = pasta
        self.urls = list(urls or [])
        self._buscar = buscar or netguard.buscar

    # ── índices ───────────────────────────────────────────────────────────────
    def _local(self) -> list[Entrada]:
        arquivo = self.pasta / "catalogo.json"
        if not arquivo.is_file():
            return []
        try:
            if arquivo.stat().st_size > MAX_INDICE:
                raise PluginError("catalog_index_too_large")
            indice = IndiceJson.model_validate_json(arquivo.read_bytes())
        except (OSError, ValueError, ValidationError):
            raise PluginError("catalog_index_invalid") from None
        return [
            Entrada(LOCAL, e.id, e.name, e.version, e.description, e.sha256, e.file or "")
            for e in indice.plugins
            if e.file
        ]

    def _remoto(self, url: str) -> list[Entrada]:
        host = urlsplit(url).hostname or "?"
        try:
            with saidas.medir(f"catalogo:{host}", "catalogo", bytes_out=len(url.encode())) as m:
                r = self._buscar(url, max_bytes=MAX_INDICE + 1)
                m.ok, m.bytes_in = r.status == 200, len(r.corpo)
        except (netguard.URLBloqueada, OSError, ValueError) as e:
            raise PluginError(f"catalog_unreachable: {type(e).__name__}") from None
        except Exception as e:  # noqa: BLE001 — falha de rede (httpx) vira erro da fonte, não do Orion
            raise PluginError(f"catalog_unreachable: {type(e).__name__}") from None
        if r.status != 200 or r.truncado or len(r.corpo) > MAX_INDICE:
            raise PluginError("catalog_index_invalid")
        try:
            indice = IndiceJson.model_validate_json(r.corpo)
        except (ValueError, ValidationError):
            raise PluginError("catalog_index_invalid") from None
        saida = []
        for e in indice.plugins:
            if not e.url:
                continue
            alvo = urljoin(r.url_final, e.url)
            if not _mesma_origem(alvo, r.url_final):
                # pacote em outro host: recusado (REGRAS.md: catálogo de plugins: baixar não
                # instala, nada atualiza sozinho)
                continue
            saida.append(Entrada(url, e.id, e.name, e.version, e.description, e.sha256, alvo))
        return saida

    def listar(self) -> tuple[list[Entrada], list[dict]]:
        """(entradas, estado de cada fonte). Fonte que falha não derruba as outras."""
        entradas: list[Entrada] = []
        fontes: list[dict] = []
        for fonte in (LOCAL, *self.urls):
            try:
                achadas = self._local() if fonte == LOCAL else self._remoto(fonte)
                entradas.extend(achadas)
                fontes.append({"fonte": fonte, "ok": True, "total": len(achadas)})
            except PluginError as e:
                fontes.append({"fonte": fonte, "ok": False, "erro": str(e)})
        return entradas, fontes

    # ── pacote ────────────────────────────────────────────────────────────────
    def baixar(self, fonte: str, id_: str, versao: str | None = None) -> tuple[Entrada, bytes]:
        """O ZIP do pacote, com o SHA-256 do índice já conferido. Não instala nada."""
        if fonte != LOCAL and fonte not in self.urls:
            raise PluginError("catalog_source_unknown")
        entradas, _ = self.listar()
        candidatas = [e for e in entradas if e.fonte == fonte and e.id == id_]
        if versao:
            candidatas = [e for e in candidatas if e.version == versao]
        if not candidatas:
            raise PluginError("catalog_entry_not_found")
        entrada = max(
            candidatas, key=lambda e: [int(x) for x in e.version.split("-")[0].split(".")]
        )
        dados = self._ler_local(entrada) if fonte == LOCAL else self._baixar_remoto(entrada)
        if hashlib.sha256(dados).hexdigest() != entrada.sha256:
            raise PluginError("catalog_hash_mismatch")
        return entrada, dados

    def _ler_local(self, e: Entrada) -> bytes:
        caminho = (self.pasta / e.destino).resolve()
        if not _ARQUIVO.match(e.destino) or caminho.parent != self.pasta.resolve():
            raise PluginError("catalog_path_invalid")
        try:
            if caminho.is_symlink() or caminho.stat().st_size > MAX_ARCHIVE:
                raise PluginError("catalog_package_invalid")
            return caminho.read_bytes()
        except OSError:
            raise PluginError("catalog_package_missing") from None

    def _baixar_remoto(self, e: Entrada) -> bytes:
        if not _mesma_origem(e.destino, e.fonte):
            raise PluginError("catalog_package_other_host")
        host = urlsplit(e.destino).hostname or "?"
        try:
            with saidas.medir(f"catalogo:{host}", "catalogo", bytes_out=len(e.destino)) as m:
                r = self._buscar(e.destino, max_bytes=MAX_ARCHIVE + 1)
                m.ok, m.bytes_in = r.status == 200, len(r.corpo)
        except Exception as err:  # noqa: BLE001 — qualquer falha de rede/barreira vira erro claro
            raise PluginError(f"catalog_unreachable: {type(err).__name__}") from None
        if r.status != 200 or r.truncado or len(r.corpo) > MAX_ARCHIVE:
            raise PluginError("catalog_package_invalid")
        return r.corpo


def _mesma_origem(a: str, b: str) -> bool:
    pa, pb = urlsplit(a), urlsplit(b)
    return (pa.scheme, pa.hostname, pa.port) == (pb.scheme, pb.hostname, pb.port) and (
        pa.scheme == "https"
    )


def escrever_indice(pasta: Path, pacotes: list[tuple[str, bytes]]) -> Path:
    """Publica ZIPs no catálogo local (usado pelo `orion plugin publicar` e pelos testes).
    Cada item é (nome do arquivo .zip, conteúdo); o índice é refeito com os dados do manifesto."""
    from .archives import archive_snapshot

    pasta.mkdir(parents=True, exist_ok=True)
    try:
        atual = json.loads((pasta / "catalogo.json").read_text(encoding="utf-8"))["plugins"]
    except (OSError, ValueError, KeyError):
        atual = []
    for nome, dados in pacotes:
        m = archive_snapshot(dados, nome).manifest
        (pasta / nome).write_bytes(dados)
        atual = [e for e in atual if not (e["id"] == m.id and e["version"] == m.version)]
        atual.append(
            {
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "description": m.description,
                "sha256": hashlib.sha256(dados).hexdigest(),
                "file": nome,
            }
        )
    (pasta / "catalogo.json").write_text(
        json.dumps({"plugins": atual}, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    return pasta / "catalogo.json"
