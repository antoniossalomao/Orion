# ADR 0008 — Token do GitHub com acesso total

**Estado:** aceita (21/10/2026, D06)

## Contexto
O Orion precisa clonar repositórios, abrir PRs e ler CI (F08, F11.06). O plano pedia reavaliar o escopo do token para o mínimo.

## Decisão
Token clássico do GitHub com acesso total (`repo`), guardado no Credential Manager. O Drive continua só com `drive.file` (arquivos criados pelo Orion).

## Consequências
Escolha do Antônio. Se o token vazar, dá acesso a todos os repositórios privados da conta; por isso fica só no cofre, nunca em arquivo, log ou commit, e a ferramenta que o usa segue as classes de risco e aprovações do `orion-policy`. Dá para trocar por token fine-grained por repositório depois, sem mudar o código.
