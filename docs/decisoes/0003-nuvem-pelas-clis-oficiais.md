# ADR 0003 — Nuvem pelas CLIs oficiais

**Estado:** aceita

## Contexto
Orion precisa de modelos fortes sem guardar credenciais de assinatura.

## Decisão
Claude e Codex rodam pelas CLIs oficiais, opt-in (`ORION_NUVEM`). Leitura de `/usage` e `/status` pelas CLIs, com queda para estimativa.

## Consequências
Termos de uso dos provedores ainda precisam ser conferidos (F00.29).
