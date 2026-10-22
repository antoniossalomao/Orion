---
name: auditoria-interface
description: Crítica de design e checagem de acessibilidade das telas do app iced do Orion. Use ao revisar mudanças em orion-app (telas, onboarding, Configurações, mini barra, overlays).
---

# Auditoria de interface do Orion

Compare com o protótipo (`Memorias Do Projeto/ORION_UI_MOCKUP.html`) e a especificação (`ORION_UI_ESPECIFICACAO.md`). Gere as fotos com `cargo snap snaps` (em `Orion_Rust/`) e olhe cada tela alterada.

## Crítica de design
- **Fidelidade:** tokens Grafite, fontes (Geist, Geist Mono, Newsreader), espaçamentos e raios do protótipo; nada de cor ou tamanho inventado.
- **Hierarquia:** um foco por tela; título, subtítulo, campo e ação principal legíveis na ordem.
- **Densidade:** grades e listas com linhas confortáveis (campo ≥ 15 px no onboarding); nada colado nas bordas.
- **Estados:** vazio, carregando, erro, desabilitado, hover e foco existem e se distinguem.
- **Texto:** pt-BR e en completos; sem texto cortado; mensagens de erro dizem o que fazer.
- **Movimento:** animações curtas e sutis; respeitar "reduzir movimento" do Windows (pendente no onboarding).

## Acessibilidade
- **Contraste:** texto normal ≥ 4,5:1 e texto grande ≥ 3:1 sobre `bg`, `side` e vidro; atenção a `fg3` e placeholders.
- **Teclado:** toda ação alcançável por Tab/Shift+Tab/Enter/Esc; ordem lógica; foco visível; atalhos globais não conflitam com leitores de tela.
- **Alvos:** botões e campos com altura mínima confortável (≥ 32 px).
- **Cor não é único sinal:** erro, sucesso e seleção também têm texto ou ícone.
- **Escala:** fontes e layout aguentam a escala do Windows (125–200%); nada transborda.
- **Leitores de tela:** iced ainda tem suporte limitado; registre o que não dá para cobrir (item "Narrador" está fora da v1).

## Saída
Lista de achados com tela, o que está errado, referência do protótipo e correção sugerida; no fim, o que não foi conferido (janela real, DPI, Windows).
