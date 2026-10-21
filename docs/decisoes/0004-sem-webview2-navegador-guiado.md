# ADR 0004 — Sem WebView2; navegador guiado

**Estado:** aceita

## Contexto
O front HTML antigo dependia de WebView.

## Decisão
Sem WebView2. Páginas são lidas em Rust (Readability + Markdown) e o navegador é guiado por CDP no Edge/Chrome instalado (`chromiumoxide`). O front HTML antigo foi removido em 21/10/2026.

## Consequências
Mermaid e HTML no chat precisam de solução própria sem WebView2 (F05.08).
