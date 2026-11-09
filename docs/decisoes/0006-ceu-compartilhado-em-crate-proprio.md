# ADR 0006 — Céu compartilhado em crate próprio

**Estado:** aceita

## Contexto
Instalador e onboarding mostram o mesmo céu animado.

## Decisão
O renderizador em CPU (estrelas, Via Láctea, nebulosa M42, constelação) vive em `orion-sky`, usado por `orion-setup` e `orion-app`.

## Consequências
Login e tela inicial usam o mesmo `orion-sky` (F01.29, `orion-app/src/ceu.rs`): 20 fps parado, 60 na interação, pausa com a janela sem foco/minimizada, quadro estático com reduzir movimento (configuração do Orion ou "Mostrar animações" do Windows). A estrela verde de voz da tela inicial deixou de existir (o motor não tem esse estado). O widget wgpu `shader` continua fora da v1.
