"""Barreira de rede para URLs que vêm do modelo (regra 7 do ORION_REGRAS.md).

O modelo monta URLs a partir de texto não confiável (páginas, e-mails, memória). Sem esta
checagem, uma injeção de prompt aponta `buscar_url` para o próprio Orion (127.0.0.1), para a
LAN, para a rede do Tailscale ou para o endpoint de metadados de uma nuvem.

Portado de `Orion_Ollama/url_guard.py` (que fica até a fase 7) e **melhorado**: o legado
resolvia o DNS, conferia e deixava a biblioteca resolver de novo na hora de conectar (janela
de DNS rebinding). Aqui o IP conferido é o IP usado: a requisição vai para o IP, com o `Host`
e o SNI do nome original (o certificado continua sendo conferido contra o nome).

Cada redirecionamento passa de novo por toda a checagem.
"""

from __future__ import annotations

import ipaddress
import socket
import ssl
from collections.abc import Callable
from dataclasses import dataclass
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

import httpx

MAX_REDIRECTS = 5
MAX_BYTES = 2_000_000
USER_AGENT = "OrionBot/1.0 (assistente pessoal)"
_TIPOS_TEXTO = (
    "text/",
    "application/json",
    "application/xml",
    "application/xhtml",
    "+json",
    "+xml",
)

Resolver = Callable[[str, int], list[str]]


class URLBloqueada(ValueError):
    """URL recusada pela política (esquema, host ou IP não público)."""


def ip_publico(ip: ipaddress.IPv4Address | ipaddress.IPv6Address) -> bool:
    if isinstance(ip, ipaddress.IPv6Address) and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    # `is_global` é False para loopback, privado, link-local, CGNAT (Tailscale), reservado
    # e não especificado.
    return ip.is_global and not ip.is_multicast


def resolver_dns(host: str, porta: int) -> list[str]:
    try:
        infos = socket.getaddrinfo(host, porta, proto=socket.IPPROTO_TCP)
    except socket.gaierror as e:
        raise URLBloqueada(f"host não resolve: {host}") from e
    return [str(i[4][0]).split("%", 1)[0] for i in infos]


@dataclass(frozen=True)
class Destino:
    """URL já validada e o IP público em que vamos conectar."""

    url: str
    host: str
    ip: str
    porta: int
    esquema: str
    caminho: str  # caminho + query


def validar(url: str, resolver: Resolver = resolver_dns) -> Destino:
    """Devolve o destino se a URL for http(s) e **todos** os IPs do host forem públicos."""
    try:
        p = urlsplit((url or "").strip())
        porta_explicita = p.port
    except ValueError as e:
        raise URLBloqueada(f"URL inválida: {e}") from e
    if p.scheme not in ("http", "https"):
        raise URLBloqueada(f"esquema não permitido: {p.scheme or 'vazio'!r} (use http/https)")
    if p.username or p.password:
        raise URLBloqueada("URL com usuário/senha embutidos")
    host = p.hostname
    if not host:
        raise URLBloqueada("URL sem host")
    porta = porta_explicita or (443 if p.scheme == "https" else 80)
    ips = resolver(host, porta)
    if not ips:
        raise URLBloqueada(f"host não resolve: {host}")
    for texto in ips:
        ip = ipaddress.ip_address(texto)
        if not ip_publico(ip):
            raise URLBloqueada(f"destino não público bloqueado: {host} -> {ip}")
    caminho = (p.path or "/") + (f"?{p.query}" if p.query else "")
    return Destino(
        url=p.geturl(), host=host, ip=ips[0], porta=porta, esquema=p.scheme, caminho=caminho
    )


def _url_do_ip(d: Destino) -> str:
    ip = f"[{d.ip}]" if ":" in d.ip else d.ip
    padrao = 443 if d.esquema == "https" else 80
    porta = "" if d.porta == padrao else f":{d.porta}"
    return f"{d.esquema}://{ip}{porta}{d.caminho}"


@dataclass(frozen=True)
class Resposta:
    status: int
    url_final: str
    tipo: str
    corpo: bytes
    truncado: bool


def buscar(
    url: str,
    *,
    resolver: Resolver = resolver_dns,
    transport: httpx.BaseTransport | None = None,
    timeout_s: float = 15.0,
    max_bytes: int = MAX_BYTES,
    verify: ssl.SSLContext | bool = True,
) -> Resposta:
    """GET seguro: valida a URL e cada redirecionamento, conecta no IP conferido, limita o
    tamanho. `transport` e `verify` (CA própria) existem para os testes."""
    atual = url
    with httpx.Client(
        transport=transport, timeout=timeout_s, follow_redirects=False, verify=verify
    ) as cliente:
        for _ in range(MAX_REDIRECTS + 1):
            d = validar(atual, resolver)
            req = cliente.build_request(
                "GET",
                _url_do_ip(d),
                headers={"Host": _host_header(d), "User-Agent": USER_AGENT, "Accept": "*/*"},
                extensions={"sni_hostname": d.host},
            )
            r = cliente.send(req, stream=True)
            try:
                if r.is_redirect:
                    destino = r.headers.get("location")
                    if not destino:
                        raise URLBloqueada("redirecionamento sem destino")
                    atual = urljoin(d.url, destino)
                    continue
                corpo, truncado = bytearray(), False
                for pedaco in r.iter_bytes():
                    corpo += pedaco
                    if len(corpo) > max_bytes:
                        corpo, truncado = corpo[:max_bytes], True
                        break
                tipo = r.headers.get("content-type", "").split(";")[0].strip().lower()
                return Resposta(r.status_code, d.url, tipo, bytes(corpo), truncado)
            finally:
                r.close()
    raise URLBloqueada(f"redirecionamentos demais (>{MAX_REDIRECTS})")


def _host_header(d: Destino) -> str:
    padrao = 443 if d.esquema == "https" else 80
    return d.host if d.porta == padrao else f"{d.host}:{d.porta}"


def e_texto(tipo: str) -> bool:
    return not tipo or any(tipo.startswith(t) or tipo.endswith(t) for t in _TIPOS_TEXTO)


_IGNORAR = frozenset({"script", "style", "noscript", "template", "svg", "head"})
_QUEBRA = frozenset(
    {"p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article"}
)


class _HtmlParaTexto(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._partes: list[str] = []
        self._fora = 0
        self.titulo = ""
        self._no_titulo = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "title":
            self._no_titulo = True
        if tag in _IGNORAR:
            self._fora += 1
        elif tag in _QUEBRA:
            self._partes.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag == "title":
            self._no_titulo = False
        if tag in _IGNORAR and self._fora:
            self._fora -= 1
        elif tag in _QUEBRA:
            self._partes.append("\n")

    def handle_data(self, data: str) -> None:
        if self._no_titulo:
            self.titulo += data
        elif not self._fora:
            self._partes.append(data)

    def texto(self) -> str:
        bruto = "".join(self._partes)
        linhas = [" ".join(ln.split()) for ln in bruto.splitlines()]
        saida: list[str] = []
        for ln in linhas:
            if ln or (saida and saida[-1]):
                saida.append(ln)
        return "\n".join(saida).strip()


def html_para_texto(html: str) -> tuple[str, str]:
    """(título, texto legível) de um HTML: sem script, estilo nem marcação."""
    p = _HtmlParaTexto()
    p.feed(html)
    p.close()
    return " ".join(p.titulo.split()), p.texto()
