# ADR 0004 — Sem WebView2; navegador guiado

**Estado:** aceita

## Contexto
O front HTML antigo dependia de WebView.

## Decisão
Sem WebView2. Páginas são lidas em Rust (Readability + Markdown) e o navegador é guiado por CDP no Edge/Chrome instalado (`chromiumoxide`). O front HTML antigo foi removido em 21/10/2026.

## Consequências
Mermaid e HTML no chat precisam de solução própria sem WebView2 (F05.08).

## Atualização (11–12/11/2026, PR #103)
O navegador guiado abre **com janela** por padrão (modo visual: cursor animado e destaque do clique) e roda **headless** com `silencioso: true` em `navegador_ler` ou `ORION_BROWSER_SILENT=1`/`ORION_BROWSER_HEADLESS=1`; trocar de modo fecha o navegador aberto (perde abas e login da sessão). Em `navegador_agir`, `silencioso` só tira a animação daquela ação. A página continua sendo conteúdo externo: é dado, nunca instrução.
