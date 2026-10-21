# ADR 0007 — Testes e CI pausados

**Estado:** aceita

## Contexto
O workflow Windows levava ~30 min e falhava (4 testes de `orion-vault` quebram no Windows).

## Decisão
Decisão de 21/10/2026: sem testes novos e sem CI por ora. O workflow `rust-windows.yml` só roda manualmente; itens F00.07, F00.08, F00.09 e F00.12 foram cancelados no plano.

## Consequências
Nada valida PRs automaticamente; reativar é trabalho futuro (religar o gatilho e corrigir os testes do vault no Windows).
