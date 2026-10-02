"""S4: o LLM não pode apontar buscar_url para o próprio Orion, a LAN ou o Tailscale."""

import socket
import types

import pytest
import requests

import url_guard
from url_guard import URLBloqueada, get_seguro, validar_url_publica


def dns(monkeypatch, mapa):
    def falso(host, porta, **_):
        if host not in mapa:
            raise socket.gaierror(host)
        return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (mapa[host], porta))]
    monkeypatch.setattr(url_guard.socket, "getaddrinfo", falso)


@pytest.mark.parametrize("url", [
    "http://127.0.0.1:8000/historico", "http://[::1]/", "http://[::ffff:127.0.0.1]/",
    "http://10.0.0.1/", "http://192.168.0.10/", "http://100.64.0.5/",  # Tailscale (CGNAT)
    "http://169.254.169.254/latest/meta-data", "http://0.0.0.0:8000", "http://2130706433/",
    "file:///etc/passwd", "ftp://example.com/x", "gopher://x", "http:///sem-host", "",
])
def test_destinos_nao_publicos_sao_bloqueados(url):
    with pytest.raises(URLBloqueada):
        validar_url_publica(url)


def test_ip_publico_passa():
    assert validar_url_publica("https://1.1.1.1/") == "https://1.1.1.1/"


def test_hostname_que_resolve_para_loopback_e_bloqueado(monkeypatch):
    dns(monkeypatch, {"interno.example": "127.0.0.1", "publico.example": "93.184.216.34"})
    with pytest.raises(URLBloqueada):
        validar_url_publica("http://interno.example/")
    assert validar_url_publica("http://publico.example/")


def test_host_que_nao_resolve_e_bloqueado(monkeypatch):
    dns(monkeypatch, {})
    with pytest.raises(URLBloqueada):
        validar_url_publica("http://nao-existe.example/")


def test_redirect_para_destino_interno_e_bloqueado(monkeypatch):
    dns(monkeypatch, {"publico.example": "93.184.216.34", "interno.example": "127.0.0.1"})
    visitadas = []

    def get(url, **kw):
        visitadas.append(url)
        assert kw["allow_redirects"] is False
        return types.SimpleNamespace(is_redirect=True, is_permanent_redirect=False,
                                     headers={"Location": "http://interno.example/admin"})
    monkeypatch.setattr(requests, "get", get)
    with pytest.raises(URLBloqueada):
        get_seguro("http://publico.example/")
    assert visitadas == ["http://publico.example/"]  # nunca chegou a pedir o destino interno


def test_redirect_relativo_e_seguido_e_limite_de_saltos(monkeypatch):
    dns(monkeypatch, {"publico.example": "93.184.216.34"})
    chamadas = []

    def get(url, **kw):
        chamadas.append(url)
        if url.endswith("/a"):
            return types.SimpleNamespace(is_redirect=True, is_permanent_redirect=False,
                                         headers={"Location": "/b"})
        return types.SimpleNamespace(is_redirect=False, is_permanent_redirect=False, headers={}, ok=True)
    monkeypatch.setattr(requests, "get", get)
    assert get_seguro("http://publico.example/a").ok
    assert chamadas == ["http://publico.example/a", "http://publico.example/b"]

    monkeypatch.setattr(requests, "get", lambda url, **kw: types.SimpleNamespace(
        is_redirect=True, is_permanent_redirect=False, headers={"Location": "/loop"}))
    with pytest.raises(URLBloqueada, match="redirecionamentos"):
        get_seguro("http://publico.example/x")
