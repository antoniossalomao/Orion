"""Concessão de automações suas (regra 58): rotinas e regras de Downloads.

Uma automação é um arquivo que **você** escreveu. Ela só roda depois de `conceder`, que guarda o
SHA-256 do arquivo inteiro: qualquer edição (sua ou de outro programa) derruba a concessão até você
conceder de novo. A concessão mora no banco (`meta`), fora do arquivo, para o arquivo não poder se
conceder sozinho.
"""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any

PREFIXO = "automacao:"


def hash_do_arquivo(arquivo: Path) -> str:
    return hashlib.sha256(arquivo.read_bytes()).hexdigest()


class Concessoes:
    def __init__(self, memory: Any) -> None:
        self._m = memory

    def _chave(self, tipo: str, nome: str) -> str:
        return f"{PREFIXO}{tipo}:{nome}"

    def conceder(self, tipo: str, nome: str, hash_: str) -> None:
        self._m.meta_set(self._chave(tipo, nome), hash_)

    def revogar(self, tipo: str, nome: str) -> bool:
        existia = self._m.meta_get(self._chave(tipo, nome)) is not None
        self._m.meta_set(self._chave(tipo, nome), None)
        return existia

    def estado(self, tipo: str, nome: str, hash_atual: str) -> str:
        """`concedida`, `mudou` (já foi concedida, o arquivo mudou) ou `sem_concessao`."""
        guardado = self._m.meta_get(self._chave(tipo, nome))
        if guardado is None:
            return "sem_concessao"
        return "concedida" if guardado == hash_atual else "mudou"
