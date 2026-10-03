"""Segredos: variável de ambiente primeiro, cofre do SO (keyring) depois.

Windows Credential Manager e macOS Keychain via `keyring`. Nunca registrar o
valor em log (ver `orion.policy.audit.redact`).
"""

from __future__ import annotations

import os

SERVICO = "orion"


def get_secret(nome: str) -> str | None:
    valor = os.environ.get(nome)
    if valor:
        return valor
    try:
        import keyring

        return keyring.get_password(SERVICO, nome)
    except Exception:  # noqa: BLE001 — sem backend de cofre (CI, servidor): segue sem segredo
        return None


def set_secret(nome: str, valor: str) -> None:
    import keyring

    keyring.set_password(SERVICO, nome, valor)
