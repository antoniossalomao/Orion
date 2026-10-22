# ADR 0009 — Modelos locais do instalador

**Estado:** aceita (21/10/2026, D09)

## Contexto
O instalador oferece modelos locais, todos descarregados até serem chamados.

## Decisão
Manter os 8 modelos da especificação: Whisper large-v3-turbo, Qwen 3 8B, Gemma 4 E4B, Granite 4.1 8B, Phi-4-mini 3.8B, Moondream 2, MiniCPM-V 4.5 e Mistral Small 3.2 24B, com as marcações atuais de recomendado e pré-selecionado.

## Consequências
Os requisitos de RAM/VRAM da especificação são estimativas e ainda não foram medidos no PC (spikes F00.22 e F00.25). O Mistral Small 24B só serve com GPU grande.
