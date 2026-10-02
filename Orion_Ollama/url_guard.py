"""url_guard.py — barreira anti-SSRF para URLs que vêm do LLM.

O LLM monta URLs a partir de texto não confiável (páginas, e-mails, RAG). Sem
esta checagem, um prompt injection consegue apontar `buscar_url` para o próprio
Orion (127.0.0.1:8000, REST sem login), para a LAN ou para a rede do Tailscale.

Só stdlib (importável por qualquer módulo do legado). Limite conhecido: a
resolução de DNS é checada antes da conexão (janela de DNS rebinding); a
mitigação completa fica para a política de rede da reescrita (`orion/security`).
"""

import ipaddress
import socket
from urllib.parse import urljoin, urlparse

_MAX_REDIRECTS = 5


class URLBloqueada(ValueError):
    """URL recusada pela política (esquema, host ou IP não público)."""


def _ip_publico(ip: ipaddress.IPv4Address | ipaddress.IPv6Address) -> bool:
    if isinstance(ip, ipaddress.IPv6Address) and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    # is_global é False para loopback, privado, link-local, CGNAT (Tailscale),
    # reservado e não especificado.
    return ip.is_global and not ip.is_multicast


def validar_url_publica(url: str) -> str:
    """Devolve a URL se for http(s) para um host que resolve só para IPs
    públicos; senão levanta URLBloqueada."""
    p = urlparse((url or "").strip())
    if p.scheme not in ("http", "https"):
        raise URLBloqueada(f"esquema não permitido: {p.scheme or 'vazio'!r} (use http/https)")
    host = p.hostname
    if not host:
        raise URLBloqueada("URL sem host")
    porta = p.port or (443 if p.scheme == "https" else 80)
    try:
        infos = socket.getaddrinfo(host, porta, proto=socket.IPPROTO_TCP)
    except socket.gaierror as e:
        raise URLBloqueada(f"host não resolve: {host}") from e
    for info in infos:
        ip = ipaddress.ip_address(info[4][0].split("%", 1)[0])
        if not _ip_publico(ip):
            raise URLBloqueada(f"destino não público bloqueado: {host} -> {ip}")
    return url


def get_seguro(url: str, **kwargs):
    """`requests.get` que valida a URL inicial e cada redirecionamento."""
    import requests

    kwargs["allow_redirects"] = False
    atual = validar_url_publica(url)
    for _ in range(_MAX_REDIRECTS + 1):
        resp = requests.get(atual, **kwargs)
        if resp.is_redirect or resp.is_permanent_redirect:
            destino = resp.headers.get("Location")
            if not destino:
                return resp
            atual = validar_url_publica(urljoin(atual, destino))
            continue
        return resp
    raise URLBloqueada(f"redirecionamentos demais (>{_MAX_REDIRECTS})")
