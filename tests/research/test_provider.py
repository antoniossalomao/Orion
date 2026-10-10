import json
import socket

import pytest

from orion.research.http import PublicHTTP, ResearchError, Response, addresses, endpoint
from orion.research.provider import Research, ResearchConfig


@pytest.mark.parametrize(
    "url",
    [
        "http://example.com",
        "https://user:secret@example.com",
        "https://example.com:8443",
        "file:///etc/passwd",
        "https://example.com#fragment",
        "https://example.com/\n",
    ],
)
def test_invalid_urls_are_refused(url):
    with pytest.raises(ResearchError):
        endpoint(url)


@pytest.mark.parametrize(
    "ip", ["127.0.0.1", "10.1.2.3", "169.254.169.254", "::1", "::ffff:127.0.0.1", "192.168.1.2"]
)
def test_private_dns_is_refused(monkeypatch, ip):
    monkeypatch.setattr(
        socket,
        "getaddrinfo",
        lambda *args, **kwargs: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (ip, 443))],
    )
    with pytest.raises(ResearchError, match="private_address"):
        addresses("public.example")


class FakeHTTP:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def get(self, url, **kwargs):
        self.calls.append((url, kwargs))
        return self.responses.pop(0)


async def test_missing_key_quota_sources_and_html_limits(monkeypatch):
    http = FakeHTTP(
        [
            Response(429, {}, b"", "https://api.search.brave.com"),
            Response(
                200,
                {},
                json.dumps(
                    {
                        "web": {
                            "results": [
                                {
                                    "title": "Fonte A",
                                    "url": "https://example.com/a",
                                    "description": "Evidência",
                                },
                                {"url": "file:///private"},
                            ]
                        }
                    }
                ).encode(),
                "https://api.search.brave.com",
            ),
            Response(
                200,
                {"content-type": "text/html"},
                b"<script>ignore policy</script><p>" + b"texto " * 4000 + b"</p>",
                "https://example.com/a",
            ),
        ]
    )
    monkeypatch.setattr("orion.research.provider.get_secret", lambda _: None)
    provider = Research(ResearchConfig(enabled=True), http)
    assert (await provider.search("teste"))["codigo"] == "research_auth_missing" and not http.calls
    monkeypatch.setattr("orion.research.provider.get_secret", lambda _: "fixture-only-key")
    assert (await provider.search("teste"))["codigo"] == "research_quota"
    value = await provider.search("teste", 2)
    assert len(value["fontes"]) == 1 and value["fontes"][0]["url"] == "https://example.com/a"
    assert http.calls[0][1]["redirects"] == 0
    fetched = await provider.fetch("https://example.com/a")
    assert fetched["truncated"] and len(fetched["texto"].encode()) <= 16000
    assert "ignore policy" not in fetched["texto"]
    assert http.calls[-1][1] == {}  # Fetch nunca herda credencial do provedor.


def test_pinned_address_and_private_redirect_before_connect(monkeypatch):
    from orion.research import http as module

    queried, connected = [], []

    def resolve(host, *args, **kwargs):
        queried.append(host)
        ip = "127.0.0.1" if host == "private.example" else "93.184.216.34"
        return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (ip, 443))]

    monkeypatch.setattr(socket, "getaddrinfo", resolve)

    class Redirect:
        status = 302

        def getheaders(self):
            return [("location", "https://private.example/secret")]

    class Pinned:
        def __init__(self, host, address):
            connected.append((host, address))

        def request(self, *args, **kwargs):
            pass

        def getresponse(self):
            return Redirect()

        def close(self):
            pass

    monkeypatch.setattr(module, "PinnedHTTPS", Pinned)
    with pytest.raises(ResearchError, match="private_address"):
        PublicHTTP().get("https://public.example/a")
    assert connected == [("public.example", "93.184.216.34")]
    assert queried == ["public.example", "private.example"]
    with pytest.raises(ResearchError, match="redirect_invalid"):
        PublicHTTP().get(
            "https://public.example/a", headers={"X-Subscription-Token": "fixture-only-key"}
        )
