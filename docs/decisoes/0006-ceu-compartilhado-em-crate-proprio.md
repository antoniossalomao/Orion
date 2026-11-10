# ADR 0006 — Céu compartilhado em crate próprio

**Estado:** aceita

## Contexto
Instalador e onboarding mostram o mesmo céu animado.

## Decisão
O renderizador em CPU (estrelas, Via Láctea, nebulosa M42, constelação) vive em `orion-sky`, usado por `orion-setup` e `orion-app`.

## Consequências
Login e tela inicial usam o mesmo `orion-sky` (F01.29, `orion-app/src/ceu.rs`), mas com **um quadro estático**, refeito só ao redimensionar. A primeira versão animada criava uma imagem (textura) nova a cada quadro: encheu a memória (488 MB), piscou e derrubou o app no Windows. A animação (20 fps parado, 60 na interação, pausa fora de vista, estático com reduzir movimento) volta com o widget wgpu que reaproveita uma textura (F16.06/F17). A estrela verde de voz da tela inicial deixou de existir (o motor não tem esse estado).
