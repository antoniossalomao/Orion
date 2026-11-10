# ADR 0006 — Céu compartilhado em crate próprio

**Estado:** aceita

## Contexto
Instalador e onboarding mostram o mesmo céu animado.

## Decisão
O renderizador em CPU (estrelas, Via Láctea, nebulosa M42, constelação) vive em `orion-sky`, usado por `orion-setup` e `orion-app`.

## Consequências
Login e tela inicial usam o mesmo `orion-sky` (F01.29, `orion-app/src/sky_frame.rs`) com o desenho do instalador: o quadro é composto numa thread própria, com um único quadro pendente, a resolução cai sozinha se passar de 14 ms, nada é pedido com a janela sem foco ou minimizada e "reduzir movimento" (Orion ou Windows) deixa um quadro estático. Parado roda a 20 fps; a subida para 60 fps na interação e o widget wgpu que reaproveita uma textura (F16.06/F17) seguem pendentes. A primeira versão compunha o quadro na thread da interface, escutava o mouse e usava `opacity`; no Windows piscou, subiu a 488 MB e derrubou o app. A estrela verde de voz da tela inicial deixou de existir (o motor não tem esse estado).
