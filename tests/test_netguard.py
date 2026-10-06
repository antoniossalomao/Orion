"""Barreira de rede: só host público, o IP conferido é o IP usado, redirecionamento revalidado."""

import httpx
import pytest

from orion import netguard
from orion.netguard import URLBloqueada, buscar, html_para_texto, validar

PUBLICO = "93.184.216.34"


def dns(tabela):
    return lambda host, porta: tabela.get(host, [])


RESOLVE = dns(
    {"exemplo.com": [PUBLICO], "outro.com": ["1.1.1.1"], "misto.com": [PUBLICO, "10.0.0.5"]}
)


@pytest.mark.parametrize(
    "url",
    [
        "ftp://exemplo.com/x",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://",
        "http://user:senha@exemplo.com/",
        "http://exemplo.com:99999/",
        "",
        "   ",
    ],
)
def test_esquema_host_e_credencial_invalidos(url):
    with pytest.raises(URLBloqueada):
        validar(url, RESOLVE)


@pytest.mark.parametrize(
    "ip",
    ["127.0.0.1", "10.1.2.3", "192.168.0.10", "172.16.5.5", "169.254.169.254", "100.64.0.9",
     "::1", "fe80::1", "fd00::1", "0.0.0.0", "::ffff:127.0.0.1", "224.0.0.1"],
)  # fmt: skip
def test_ip_nao_publico_e_bloqueado_inclusive_tailscale_e_metadados(ip):
    with pytest.raises(URLBloqueada, match="não público"):
        validar("http://alvo.test/", dns({"alvo.test": [ip]}))


def test_um_ip_privado_entre_os_publicos_basta_para_bloquear():
    with pytest.raises(URLBloqueada):
        validar("http://misto.com/", RESOLVE)


def test_ip_literal_privado_e_bloqueado_sem_dns():
    with pytest.raises(URLBloqueada):
        validar("http://127.0.0.1:8000/chat", netguard.resolver_dns)
    with pytest.raises(URLBloqueada):
        validar("http://[::1]/", netguard.resolver_dns)


def test_host_que_nao_resolve_e_bloqueado():
    with pytest.raises(URLBloqueada, match="não resolve"):
        validar("http://nao-existe.test/", dns({}))


def test_destino_guarda_o_ip_conferido_e_o_caminho():
    d = validar("https://exemplo.com:8443/a/b?q=1", RESOLVE)
    assert (d.ip, d.porta, d.caminho, d.host) == (PUBLICO, 8443, "/a/b?q=1", "exemplo.com")


def cliente_falso(handler):
    return httpx.MockTransport(handler)


def test_conecta_no_ip_conferido_com_host_e_sni_do_nome_original():
    vistos = []

    def h(req: httpx.Request):
        vistos.append((str(req.url), req.headers["host"], req.extensions.get("sni_hostname")))
        return httpx.Response(
            200, text="olá", headers={"content-type": "text/plain; charset=utf-8"}
        )

    r = buscar("https://exemplo.com/p?x=1", resolver=RESOLVE, transport=cliente_falso(h))
    assert r.corpo == "olá".encode() and r.tipo == "text/plain" and r.status == 200
    # a requisição vai para o IP (sem nova resolução de DNS), mas o nome segue no Host e no SNI
    assert vistos == [(f"https://{PUBLICO}/p?x=1", "exemplo.com", "exemplo.com")]


def test_ipv6_publico_vai_entre_colchetes():
    v6 = dns({"v6.test": ["2606:4700:4700::1111"]})
    vistos = []

    def h(req):
        vistos.append(str(req.url))
        return httpx.Response(200, text="ok")

    buscar("http://v6.test:8080/x", resolver=v6, transport=cliente_falso(h))
    assert vistos == ["http://[2606:4700:4700::1111]:8080/x"]


def test_cada_redirecionamento_e_revalidado_e_o_para_o_interno_cai():
    def h(req):
        if req.headers["host"] == "exemplo.com":
            return httpx.Response(302, headers={"location": "http://interno.test/admin"})
        return httpx.Response(200, text="segredo")

    resolver = dns({"exemplo.com": [PUBLICO], "interno.test": ["192.168.0.1"]})
    with pytest.raises(URLBloqueada, match="não público"):
        buscar("http://exemplo.com/", resolver=resolver, transport=cliente_falso(h))


def test_redirecionamento_publico_e_seguido_e_relativo_funciona():
    def h(req):
        if req.url.path == "/a":
            return httpx.Response(301, headers={"location": "/b"})
        if req.url.path == "/b":
            return httpx.Response(302, headers={"location": "http://outro.com/c"})
        return httpx.Response(200, text="fim", headers={"content-type": "text/plain"})

    r = buscar("http://exemplo.com/a", resolver=RESOLVE, transport=cliente_falso(h))
    assert r.corpo == b"fim" and r.url_final.startswith("http://outro.com/c")


def test_redirecionamentos_demais_e_sem_destino():
    com = lambda req: httpx.Response(302, headers={"location": "/de-novo"})  # noqa: E731
    with pytest.raises(URLBloqueada, match="demais"):
        buscar("http://exemplo.com/", resolver=RESOLVE, transport=cliente_falso(com))
    sem = lambda req: httpx.Response(302)  # noqa: E731
    with pytest.raises(URLBloqueada, match="sem destino"):
        buscar("http://exemplo.com/", resolver=RESOLVE, transport=cliente_falso(sem))


def test_corpo_grande_e_cortado_sem_baixar_tudo():
    def h(req):
        return httpx.Response(200, content=b"x" * 5000, headers={"content-type": "text/plain"})

    r = buscar("http://exemplo.com/", resolver=RESOLVE, transport=cliente_falso(h), max_bytes=1000)
    assert len(r.corpo) == 1000 and r.truncado


def test_html_vira_texto_sem_script_estilo_nem_marcacao():
    titulo, texto = html_para_texto(
        "<html><head><title> Meu  título </title><style>p{color:red}</style></head>"
        "<body><h1>Olá</h1><script>alert(1)</script><p>um <b>dois</b></p><p>três &amp; quatro</p>"
        "<noscript>ignora</noscript></body></html>"
    )
    assert titulo == "Meu título"
    assert texto == "Olá\n\num dois\n\ntrês & quatro"  # um parágrafo vazio entre blocos
    assert "alert" not in texto and "color" not in texto


def test_e_texto():
    assert netguard.e_texto("text/html") and netguard.e_texto("application/json")
    assert netguard.e_texto("application/ld+json") and netguard.e_texto("")
    assert not netguard.e_texto("image/png") and not netguard.e_texto("application/pdf")
