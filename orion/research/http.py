"""GET HTTPS público com DNS fixado por conexão e corpo limitado; sem proxy/credenciais."""

from __future__ import annotations

import contextlib
import http.client
import ipaddress
import socket
import ssl
import threading
from dataclasses import dataclass
from urllib.parse import urljoin, urlsplit


class ResearchError(ValueError):
    pass


def endpoint(url: str) -> tuple[str, str]:
    if len(url) > 4096 or any(ord(c) < 33 for c in url):
        raise ResearchError("research_url_invalid")
    try:
        parsed = urlsplit(url)
        host = (parsed.hostname or "").encode("idna").decode("ascii")
        if (
            parsed.scheme != "https"
            or not host
            or parsed.username
            or parsed.password
            or parsed.port not in (None, 443)
            or parsed.fragment
            or "%" in host
        ):
            raise ResearchError("research_url_invalid")
    except (ValueError, UnicodeError) as error:
        raise ResearchError("research_url_invalid") from error
    return host, (parsed.path or "/") + ("?" + parsed.query if parsed.query else "")


def addresses(host: str) -> list[str]:
    try:
        answers = socket.getaddrinfo(host, 443, type=socket.SOCK_STREAM)
        values = list(dict.fromkeys(str(answer[4][0]) for answer in answers))
        if not values or len(values) > 16:
            raise ResearchError("research_dns_invalid")
        for value in values:
            address = ipaddress.ip_address(value)
            if not address.is_global or (
                isinstance(address, ipaddress.IPv6Address)
                and address.ipv4_mapped
                and not address.ipv4_mapped.is_global
            ):
                raise ResearchError("research_private_address")
        return values
    except OSError as error:
        raise ResearchError("research_dns_unavailable") from error


class PinnedHTTPS(http.client.HTTPSConnection):
    def __init__(self, host: str, address: str):
        self.tls_context = ssl.create_default_context()
        super().__init__(host, timeout=5, context=self.tls_context)
        self.address = address
        self.bound_socket: ssl.SSLSocket | None = None

    def connect(self) -> None:
        # DNS não é consultado novamente: TLS valida o hostname original sobre o IP revisado.
        raw = socket.create_connection((self.address, 443), timeout=self.timeout)
        try:
            self.sock = self.tls_context.wrap_socket(raw, server_hostname=self.host)
            self.bound_socket = self.sock
        except BaseException:
            raw.close()
            raise


@dataclass(frozen=True)
class Response:
    status: int
    headers: dict[str, str]
    content: bytes
    url: str


class PublicHTTP:
    def get(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = None,
        limit: int = 1_000_000,
        redirects: int = 3,
    ) -> Response:
        current = url
        for hop in range(redirects + 1):
            host, path = endpoint(current)
            ips = addresses(host)
            connection = PinnedHTTPS(host, ips[0])

            def interrupt(conn=connection) -> None:
                bound = getattr(conn, "bound_socket", None)
                if bound is not None:
                    with contextlib.suppress(OSError):
                        bound.shutdown(socket.SHUT_RDWR)
                conn.close()

            deadline = threading.Timer(15, interrupt)
            deadline.daemon = True
            deadline.start()
            try:
                connection.request(
                    "GET",
                    path,
                    headers={
                        "User-Agent": "Orion/0.1 (research)",
                        "Accept-Encoding": "identity",
                        **(headers or {}),
                    },
                )
                response = connection.getresponse()
                info = {name.lower(): value for name, value in response.getheaders()}
                if sum(len(k) + len(v) for k, v in info.items()) > 64000:
                    raise ResearchError("research_headers_limit")
                if response.status in {301, 302, 303, 307, 308}:
                    if headers or hop == redirects or "location" not in info:
                        raise ResearchError("research_redirect_invalid")
                    current = urljoin(current, info["location"])
                    endpoint(current)
                    continue
                if info.get("content-encoding", "identity").lower() not in {"", "identity"}:
                    raise ResearchError("research_encoding_unsupported")
                data = response.read(limit + 1)
                if len(data) > limit:
                    raise ResearchError("research_size_limit")
                return Response(response.status, info, data, current)
            except (OSError, http.client.HTTPException) as error:
                raise ResearchError("research_unavailable") from error
            finally:
                deadline.cancel()
                connection.close()
        raise ResearchError("research_redirect_invalid")
