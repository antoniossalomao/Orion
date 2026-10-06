"""Login do Orion (fase 5): senha com PBKDF2 e sessão por cookie opaco e revogável.

Uma pessoa só (o Antônio), então há uma credencial e várias sessões (notebook, celular).
O token da sessão é aleatório (256 bits); o banco guarda só o SHA-256 dele, então vazar o
arquivo não dá acesso. Não é JWT: uma sessão opaca pode ser revogada na hora (logout, troca
de senha), o que um JWT sem estado não permite.

Os dados ficam em `auth.db`, **fora** do banco da memória: o backup diário vai para a
nuvem (iCloud/OneDrive) e não deve levar o hash da senha. Restaurar um backup num
notebook novo exige `orion set-password` de novo, de propósito.

Tentativas erradas travam por cliente (e no total), com tempo crescente: ver `LoginThrottle`.
"""

from __future__ import annotations

import base64
import contextlib
import hashlib
import hmac
import secrets
import sqlite3
import threading
import time
from collections import defaultdict, deque
from collections.abc import Callable
from pathlib import Path

ALGORITMO = "pbkdf2_sha256"
ITERACOES = 600_000  # OWASP 2023 para PBKDF2-HMAC-SHA256
SENHA_MIN = 12
SENHA_MAX = 256

_DDL = """
CREATE TABLE IF NOT EXISTS credential (
    user TEXT PRIMARY KEY,
    pw_hash TEXT NOT NULL,
    updated_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    user TEXT NOT NULL,
    created_at REAL NOT NULL,
    expires_at REAL NOT NULL
);
"""


class AuthError(Exception):
    """Base dos erros de login."""


class NotConfigured(AuthError):
    """Ainda não há senha (rode `orion set-password`)."""


class BadCredentials(AuthError):
    """Senha errada."""


class LockedOut(AuthError):
    def __init__(self, retry_after: int) -> None:
        super().__init__(f"muitas tentativas; tente de novo em {retry_after}s")
        self.retry_after = retry_after


class WeakPassword(AuthError):
    """Senha curta demais (ou longa demais)."""


def hash_password(senha: str, iteracoes: int | None = None) -> str:
    iteracoes = iteracoes or ITERACOES
    sal = secrets.token_bytes(16)
    h = hashlib.pbkdf2_hmac("sha256", senha.encode(), sal, iteracoes)
    return f"{ALGORITMO}${iteracoes}${_b64(sal)}${_b64(h)}"


def verify_password(senha: str, armazenado: str) -> bool:
    try:
        algoritmo, it, sal, esperado = armazenado.split("$")
        if algoritmo != ALGORITMO:
            return False
        h = hashlib.pbkdf2_hmac("sha256", senha.encode(), _unb64(sal), int(it))
    except ValueError:
        return False
    return hmac.compare_digest(h, _unb64(esperado))


def _b64(b: bytes) -> str:
    return base64.b64encode(b).decode()


def _unb64(s: str) -> bytes:
    return base64.b64decode(s.encode(), validate=True)


def _digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


class LoginThrottle:
    """Trava quem erra a senha: `max_falhas` na janela travam o cliente por `trava_s`, que
    dobra a cada nova rodada de erros (até `trava_max_s`). Há também um teto global (vários
    clientes não contornam o limite trocando de IP). Em memória: reiniciar o Orion destrava."""

    def __init__(
        self,
        *,
        max_falhas: int = 5,
        janela_s: float = 900,
        trava_s: float = 60,
        trava_max_s: float = 3600,
        max_globais: int = 30,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self._max, self._janela = max_falhas, janela_s
        self._trava, self._trava_max = trava_s, trava_max_s
        self._max_glob = max_globais
        self._clock = clock
        self._falhas: dict[str, deque[float]] = defaultdict(deque)
        self._globais: deque[float] = deque()
        self._ate: dict[str, float] = {}
        self._rodadas: dict[str, int] = defaultdict(int)
        self._lock = threading.Lock()

    def _podar(self, fila: deque[float], agora: float) -> None:
        while fila and agora - fila[0] > self._janela:
            fila.popleft()

    def espera(self, cliente: str) -> int:
        """Segundos até poder tentar de novo (0 = liberado)."""
        agora = self._clock()
        with self._lock:
            self._podar(self._globais, agora)
            if len(self._globais) >= self._max_glob:
                return max(1, int(self._janela - (agora - self._globais[0])))
            return max(0, int(self._ate.get(cliente, 0) - agora + 0.999))

    def falhou(self, cliente: str) -> None:
        agora = self._clock()
        with self._lock:
            fila = self._falhas[cliente]
            self._podar(fila, agora)
            self._podar(self._globais, agora)
            fila.append(agora)
            self._globais.append(agora)
            if len(fila) >= self._max:
                self._rodadas[cliente] += 1
                trava = min(self._trava * 2 ** (self._rodadas[cliente] - 1), self._trava_max)
                self._ate[cliente] = agora + trava
                fila.clear()

    def acertou(self, cliente: str) -> None:
        with self._lock:
            self._falhas.pop(cliente, None)
            self._ate.pop(cliente, None)
            self._rodadas.pop(cliente, None)


class AuthService:
    def __init__(
        self,
        path: Path | str,
        *,
        user: str = "antonio",
        ttl_s: float = 7 * 86400,
        iteracoes: int | None = None,
        throttle: LoginThrottle | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.user = user
        self._ttl = ttl_s
        self._it = iteracoes or ITERACOES  # lido na hora: os testes baixam o custo
        self._clock = clock
        self.throttle = throttle or LoginThrottle()
        self._lock = threading.RLock()
        caminho = Path(path)
        caminho.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(str(caminho), check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.executescript(_DDL)
        with contextlib.suppress(OSError):  # só o dono lê (no Windows o chmod quase não faz nada)
            caminho.chmod(0o600)
        # hash falso para igualar o tempo de "usuário sem senha" ao de "senha errada"
        self._falso = hash_password(secrets.token_urlsafe(16), self._it)

    def close(self) -> None:
        with self._lock:
            self._conn.close()

    # ── credencial ────────────────────────────────────────────────────────
    def has_password(self) -> bool:
        with self._lock:
            return self._conn.execute("SELECT 1 FROM credential").fetchone() is not None

    def set_password(self, senha: str) -> None:
        """Define ou troca a senha e **revoga todas as sessões** (quem estava dentro sai)."""
        if len(senha) < SENHA_MIN:
            raise WeakPassword(f"a senha precisa de pelo menos {SENHA_MIN} caracteres")
        if len(senha) > SENHA_MAX:
            raise WeakPassword(f"a senha passa de {SENHA_MAX} caracteres")
        if senha.casefold() == self.user.casefold():
            raise WeakPassword("a senha não pode ser o nome de usuário")
        novo = hash_password(senha, self._it)
        with self._lock, self._conn:
            self._conn.execute(
                "INSERT INTO credential(user, pw_hash, updated_at) VALUES (?,?,?)"
                " ON CONFLICT(user) DO UPDATE SET pw_hash=excluded.pw_hash,"
                " updated_at=excluded.updated_at",
                (self.user, novo, self._clock()),
            )
            self._conn.execute("DELETE FROM sessions")

    def check_password(self, senha: str) -> bool:
        """Confere a senha sem criar sessão nem contar tentativa (use `login`)."""
        with self._lock:
            r = self._conn.execute(
                "SELECT pw_hash FROM credential WHERE user=?", (self.user,)
            ).fetchone()
        ok = verify_password(senha, r[0] if r else self._falso)
        return ok and r is not None

    # ── sessões ───────────────────────────────────────────────────────────
    def login(self, senha: str, cliente: str = "?", usuario: str | None = None) -> str:
        """Devolve o token da sessão (vai no cookie) ou levanta `AuthError`. `usuario` ausente
        vale para o usuário único; presente e errado falha **igual** a senha errada (e a senha é
        conferida mesmo assim, para não revelar por tempo qual dos dois errou)."""
        if not self.has_password():
            raise NotConfigured("defina a senha com: orion set-password")
        espera = self.throttle.espera(cliente)
        if espera:
            raise LockedOut(espera)
        usuario_ok = usuario is None or hmac.compare_digest(
            usuario.strip().casefold().encode(), self.user.casefold().encode()
        )
        senha_ok = len(senha) <= SENHA_MAX and self.check_password(senha)
        if not (usuario_ok and senha_ok):
            self.throttle.falhou(cliente)
            raise BadCredentials("usuário ou senha incorretos")
        self.throttle.acertou(cliente)
        token = secrets.token_urlsafe(32)
        agora = self._clock()
        with self._lock, self._conn:
            self._conn.execute("DELETE FROM sessions WHERE expires_at < ?", (agora,))
            self._conn.execute(
                "INSERT INTO sessions(token_hash, user, created_at, expires_at) VALUES (?,?,?,?)",
                (_digest(token), self.user, agora, agora + self._ttl),
            )
        return token

    def validate(self, token: str | None) -> str | None:
        """Usuário dono do token, ou None se inexistente, revogado ou expirado."""
        if not token or len(token) > 200:
            return None
        with self._lock:
            r = self._conn.execute(
                "SELECT user, expires_at FROM sessions WHERE token_hash=?", (_digest(token),)
            ).fetchone()
        if r is None or r[1] < self._clock():
            return None
        return str(r[0])

    def logout(self, token: str | None) -> None:
        if token:
            with self._lock, self._conn:
                self._conn.execute("DELETE FROM sessions WHERE token_hash=?", (_digest(token),))

    def active_sessions(self) -> int:
        with self._lock:
            return int(
                self._conn.execute(
                    "SELECT COUNT(*) FROM sessions WHERE expires_at >= ?", (self._clock(),)
                ).fetchone()[0]
            )

    def revoke_all(self) -> int:
        with self._lock, self._conn:
            return self._conn.execute("DELETE FROM sessions").rowcount
