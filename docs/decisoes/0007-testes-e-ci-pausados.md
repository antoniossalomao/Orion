# ADR 0007 — Testes e CI pausados

**Estado:** substituída em 26/11/2026 (CI reativado, #109)

## Contexto
O workflow Windows levava ~30 min e falhava (4 testes de `orion-vault` quebram no Windows).

## Decisão
Decisão de 21/10/2026: sem testes novos e sem CI por ora. O workflow `rust-windows.yml` só rodava manualmente e foi removido em 03/11/2026, a pedido do Antônio (junto com `release.yml`, recriado em 06/11/2026 para o release assinado; dispara só por tag `v*` ou manual, sem testes); itens F00.07, F00.08, F00.09 e F00.12 foram cancelados no plano.

## Consequências
Nada valida PRs automaticamente; reativar é trabalho futuro (religar o gatilho e corrigir os testes do vault no Windows).


## Revisão de 26/11/2026
O CI voltou: `rust-windows.yml` roda em todo PR que mexe em `Orion_Rust/` (testes Windows, `cargo deny`, `cargo audit`). Advisories sem correção imediata ficam como exceções com motivo em `Orion_Rust/deny.toml` e `.cargo/audit.toml` (dívida no #135). Regra: CI verde para mergear (`REGRAS.md`); a proteção da `main` com os checks obrigatórios é o #111.
