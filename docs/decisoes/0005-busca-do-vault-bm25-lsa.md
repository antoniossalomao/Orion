# ADR 0005 — Busca do vault: BM25 + LSA

**Estado:** aceita

## Contexto
Busca precisa funcionar offline e rápido nas notas.

## Decisão
BM25 com LSA; embeddings locais só se a busca falhar.

## Consequências
Índice incremental completo ainda pendente (F06).
