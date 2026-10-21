# ADR 0006 — Céu compartilhado em crate próprio

**Estado:** aceita

## Contexto
Instalador e onboarding mostram o mesmo céu animado.

## Decisão
O renderizador em CPU (estrelas, Via Láctea, nebulosa M42, constelação) vive em `orion-sky`, usado por `orion-setup` e `orion-app`.

## Consequências
Widget wgpu `shader` e uso em login/tela inicial ficam pendentes (F01.29).
