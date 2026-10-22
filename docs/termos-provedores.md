# Termos de uso dos provedores (F00.29)

Verificação feita em **21/10/2026** em páginas oficiais, para decidir como o Orion pode usar assinaturas e CLIs em segundo plano e ler `/usage` e `/status` (D03). **Não é parecer jurídico**: é o que as páginas dizem e a leitura prática que fizemos. Refazer se o provedor mudar os termos.

## Claude (Anthropic)
Fonte: [Claude Code, Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance).
- Uso do Claude Code com Free, Pro e Max segue os Consumer Terms; Team, Enterprise e API seguem os Commercial Terms, e tudo está sujeito à Usage Policy.
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."
- OAuth é "intended exclusively for purchasers" das assinaturas e "designed to support ordinary use of Claude Code and other native Anthropic applications".
- Quem constrói produtos (inclusive com o Agent SDK) deve usar chave de API. A Anthropic "does not permit" rotear pedidos por credenciais Free/Pro/Max em nome de outros usuários, nem coletar, guardar ou intermediar credenciais ou tokens de sessão do Claude.ai.
- Nada impede o usuário final de entrar com a própria assinatura no binário do Claude Code **sem modificação**.

**Leitura para o Orion:** chamar a CLI oficial, sem modificá-la, com a assinatura do próprio Antônio, para uso pessoal, não é vedado. O Orion **nunca pode ler, copiar ou guardar** o token OAuth. Ler `/usage` rodando o comando da CLI é uso da própria CLI. Automação pesada em segundo plano com assinatura é zona cinzenta ("ordinary, individual usage"): manter opt-in, com os tetos de orçamento (95% semanal, 20% por tarefa), e preferir chave de API para qualquer coisa contínua.

## Codex (OpenAI)
Fontes: [Codex, autenticação](https://learn.chatgpt.com/docs/auth.md) e [Codex, preços e limites](https://learn.chatgpt.com/docs/pricing.md).
- Dois modos: entrar com ChatGPT (assinatura) ou com chave de API (por uso). "API keys are still the recommended default for automation." e "Use API key authentication for programmatic Codex CLI workflows, such as CI/CD jobs."
- Tokens de acesso do Codex para automação ("trusted scripts, schedulers, and private CI runners") aparecem só nos planos Business e Enterprise, não em Plus ou Pro.
- `~/.codex/auth.json` deve ser tratado "like a password"; não compartilhar nem commitar.
- "If you want to see your remaining limits during an active Codex CLI session, you can use `/status`."
- As páginas **não dizem** se credenciais ChatGPT podem ser usadas em fluxos automatizados ou por terceiros.

**Leitura para o Orion:** ler `/status` pela CLI é o caminho documentado. O Orion não lê nem copia o `auth.json`. Para automação contínua, a recomendação oficial é chave de API; uso com assinatura segue opt-in e limitado.

## Gemini (Google)
Fonte: [Gemini CLI, Terms of Service and Privacy Notice](https://geminicli.com/docs/resources/tos-privacy).
- Acessar os serviços do Gemini CLI com software de terceiros, "such as using OpenClaw with Gemini CLI OAuth", "is a violation of applicable terms and policies" e pode levar à suspensão da conta.
- Os termos dependem de como se entra: conta Google pessoal (Gemini Code Assist for individuals), chave do Gemini Developer API (Gemini API Terms) ou Vertex AI (termos do Google Cloud).
- Estatísticas de uso da CLI podem ser desligadas por configuração.

**Leitura para o Orion:** nunca usar o OAuth do Gemini CLI a partir do Orion. Para Gemini, usar chave de API (Google AI Studio), que já é o caminho do gateway. Conforme D03, o uso do Gemini é só estimativa local.

## Decisões práticas
1. O Orion só chama as CLIs oficiais como processos, sem modificá-las, e **nunca lê, copia, guarda ou repassa tokens OAuth** (Claude, Codex ou Gemini).
2. Leitura de uso: `/usage` (Claude) e `/status` (Codex) pela CLI; Gemini por estimativa. Se o formato mudar, cai para a estimativa e avisa (já decidido em D03).
3. Segundo plano contínuo ou de alto volume: preferir chave de API; assinatura só opt-in e dentro dos tetos.
4. Nada disso autoriza revender ou intermediar acesso a terceiros; o Orion é de uso pessoal (D07).
5. Revisar este arquivo a cada mudança de termos dos provedores ou antes de ligar qualquer uso automático novo (F03.26).
