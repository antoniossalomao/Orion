"""Regressão B1: o Telegram devolvia o JSON cru do SSE em vez do texto."""

import json

from orion_telegram import _extrair_texto


def sse(*eventos):
    return [
        f"data: {json.dumps(e)}\n\n".encode()
        if not isinstance(e, str)
        else f"data: {e}\n\n".encode()
        for e in eventos
    ]


def test_junta_so_o_texto_e_ignora_tier_e_done():
    linhas = sse({"tier": "Groq"}, {"text": "Olá, "}, {"text": "Antônio."}, "[DONE]")
    assert _extrair_texto(linhas) == "Olá, Antônio."


def test_preserva_quebras_de_linha_do_texto():
    assert _extrair_texto(sse({"text": "a\nb"})) == "a\nb"


def test_linhas_invalidas_e_vazias_sao_ignoradas():
    linhas = [b"\n", b": keepalive\n", b"data: nao-e-json\n", *sse({"text": "ok"}), b"data: \n"]
    assert _extrair_texto(linhas) == "ok"


def test_sem_texto_devolve_vazio():
    assert _extrair_texto(sse({"tier": "Gemini"}, "[DONE]")) == ""
