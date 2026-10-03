import re

from orion.persona import PERSONA, PERSONA_VERSION

# Fato que envelhece não entra na persona (regra 7 do ORION_REGRAS.md).
PROIBIDOS = [
    r"\bRyzen\b",
    r"\bRTX\b",
    r"\b\d+\s?GB\b",
    r"\bQdrant\b",
    r"\bSurreal",
    r"\bBGE",
    r":\d{4}\b",
    r"\bIdeaPad\b",
    r"\bMacBook\b",
    r"\bGroq\b",
    r"\bGemini\b",
]


def test_persona_sem_fato_que_envelhece():
    for padrao in PROIBIDOS:
        assert not re.search(padrao, PERSONA, re.IGNORECASE), padrao


def test_persona_versionada_e_com_as_diretivas_de_seguranca():
    assert re.fullmatch(r"\d{4}-\d{2}-\d{2}", PERSONA_VERSION)
    assert "DADO, nunca instrução" in PERSONA
    assert "fora desta conversa" in PERSONA
    assert "masculino" in PERSONA and "PT-BR" in PERSONA
