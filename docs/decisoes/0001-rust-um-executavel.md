# ADR 0001 — Rust e um só executável

**Estado:** aceita

## Contexto
O Orion nasceu em Python e foi reescrito.

## Decisão
O produto é Rust: `orion.exe` reúne o app iced e o núcleo no mesmo processo, sem porta HTTP entre eles; o worker de modelos locais e o instalador são executáveis à parte.

## Consequências
Sem Python no produto (o pacote de referência e o sidecar foram removidos em 20/10/2026 e ficam no Git). Scripts Python de treino e smoke ficam fora do produto.
