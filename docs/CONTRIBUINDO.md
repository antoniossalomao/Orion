# Contribuindo

Projeto pessoal e privado; estas regras valem para o Antônio e para qualquer agente de IA que trabalhe no repositório.

1. **Leia antes:** `AGENTS.md`, [REGRAS.md](../Memorias%20Do%20Projeto/REGRAS.md) e [ORION.md](../Memorias%20Do%20Projeto/ORION.md). O núcleo imutável (`orion-policy`, `REGRAS.md`, persona) não é editado por agente.
2. **Branch e PR:** uma branch por fase do board (ex.: `fase-f01-ui`), PR obrigatório para a `main`, sem merge automático. Descreva o que mudou, o que foi validado e o que não foi.
3. **Commits:** mensagem curta no imperativo, em português; explique o porquê no corpo quando não for óbvio.
4. **Código:** siga o estilo do crate. Nomes de código em inglês; comentários só onde ajudam, em português. Código já existente não é renomeado só para seguir esta regra. Sem dependências novas sem motivo; `Cargo.lock` sempre versionado.
5. **Documentação:** o repositório é a origem; ao mudar o estado do projeto, atualize `Memorias Do Projeto/ORION.md` (e a auditoria de pendências) e espelhe no vault na mesma sessão.
6. **Segredos:** nunca em arquivo, log ou commit. Use o cofre do sistema e `.env.example` só com nomes.
7. **Testes e CI:** pausados desde 21/10/2026 ([ADR 0007](decisoes/0007-testes-e-ci-pausados.md)). Não crie testes novos nem reative workflows sem pedido.
8. **Revisão cruzada:** Claude Code Review e Codex comentam nos PRs. Achado vermelho (P0/P1) se corrige; sugestão opcional se responde em uma linha.
9. **Branch protegida na `main`:** configuração do GitHub (Settings › Branches), feita à mão pelo dono do repositório; não é versionada.

## Checklist de função nova com UI

Antes de considerar pronta qualquer função com tela ou componente (K06 do board):

- [ ] Estado vazio com ilustração ou texto e uma ação sugerida.
- [ ] Estado de erro com mensagem que diga o que fazer.
- [ ] Foco de teclado: ordem lógica, foco visível, nada preso.
- [ ] Atalho de teclado quando couber.
- [ ] Textos em português do Brasil e em inglês (Fluent).
