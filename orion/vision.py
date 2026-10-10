"""Visão: pergunta sobre uma imagem ao mesmo gateway de modelos (API compatível com a da OpenAI).

O `ChatGateway` é assíncrono e em streaming; as ferramentas rodam numa thread e querem a resposta
inteira, então aqui há um cliente síncrono pequeno, sem ferramentas e sem histórico: uma imagem, uma
pergunta, um texto. Só funciona se o modelo do gateway aceitar imagem (`ORION_VISION_MODEL` troca o
modelo só para visão).

O texto que volta descreve o que está na imagem, e a imagem é conteúdo não confiável (uma página, um
e-mail ou uma foto pode trazer "ignore as instruções anteriores" escrito): quem chama trata o
resultado como conteúdo externo (regra 4). Chave só em cabeçalho; nenhum erro leva a URL nem a
chave.
"""

from __future__ import annotations

import base64
import json
from typing import Any

import httpx

from . import saidas
from .gateway import Endpoint

MAX_IMAGEM = 6 * 1024 * 1024  # bytes; imagem maior que isso o provedor gratuito costuma recusar
MAX_RESPOSTA = 6000
PERGUNTA_PADRAO = "Descreva o que você vê, em português, e transcreva o texto legível."
MIMES = {
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".webp": "image/webp",
    ".gif": "image/gif",
}


class VisionError(RuntimeError):
    """Falha ao perguntar sobre a imagem. A mensagem nunca carrega URL nem chave."""


class Vision:
    def __init__(self, endpoints: list[Endpoint]) -> None:
        if not endpoints:
            raise ValueError("ao menos um endpoint")
        self.endpoints = endpoints

    def describe(
        self,
        imagem: bytes,
        mime: str,
        pergunta: str = "",
        transport: httpx.BaseTransport | None = None,
    ) -> str:
        if not imagem:
            raise VisionError("imagem vazia")
        if len(imagem) > MAX_IMAGEM:
            raise VisionError(f"imagem passa de {MAX_IMAGEM // 1024 // 1024} MB")
        url_dados = f"data:{mime};base64,{base64.b64encode(imagem).decode('ascii')}"
        mensagens = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": pergunta.strip()[:2000] or PERGUNTA_PADRAO},
                    {"type": "image_url", "image_url": {"url": url_dados}},
                ],
            }
        ]
        falhas: list[str] = []
        with httpx.Client(transport=transport) as cliente:
            for ep in self.endpoints:
                try:
                    return self._perguntar(cliente, ep, mensagens)
                except VisionError as e:
                    falhas.append(f"{ep.name}: {e}")
        raise VisionError("; ".join(falhas) or "nenhum endpoint respondeu")

    @staticmethod
    def _perguntar(cliente: httpx.Client, ep: Endpoint, mensagens: list[dict[str, Any]]) -> str:
        corpo = {"model": ep.model, "messages": mensagens, "stream": False}
        with saidas.medir(
            "gateway:visao",
            "vision",
            model=ep.model,
            bytes_out=len(json.dumps(corpo).encode()),
            content_kind="imagem",
        ) as m:  # a imagem sai do computador: fica no registro de saída (regra 47)
            texto = Vision._enviar(cliente, ep, corpo, m)
            m.ok = True
        return texto

    @staticmethod
    def _enviar(
        cliente: httpx.Client, ep: Endpoint, corpo: dict[str, Any], m: saidas.Medida
    ) -> str:
        headers = {"Authorization": f"Bearer {ep.api_key}"} if ep.api_key else {}
        url = ep.base_url.rstrip("/") + "/chat/completions"
        try:
            resp = cliente.post(url, json=corpo, headers=headers, timeout=ep.timeout_s)
        except httpx.HTTPError as e:
            raise VisionError(type(e).__name__) from None  # a mensagem do httpx leva a URL
        m.bytes_in = len(resp.content)
        if resp.status_code >= 400:
            raise VisionError(f"HTTP {resp.status_code}")
        try:
            texto = resp.json()["choices"][0]["message"]["content"]
        except (ValueError, KeyError, IndexError, TypeError):
            raise VisionError("resposta em formato inesperado") from None
        if isinstance(texto, list):  # alguns provedores devolvem partes de conteúdo
            texto = "".join(str(p.get("text", "")) for p in texto if isinstance(p, dict))
        texto = str(texto or "").strip()
        if not texto:
            raise VisionError("resposta vazia (o modelo aceita imagem?)")
        return texto[:MAX_RESPOSTA]
