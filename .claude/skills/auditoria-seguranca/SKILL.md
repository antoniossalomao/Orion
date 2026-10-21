---
name: auditoria-seguranca
description: Revisão de segurança de uma mudança no Orion (agente com ferramentas, vault, cofre, rede). Use ao revisar PRs que mexem em ferramentas, aprovação, shell, caminhos, credenciais, MCP, Telegram ou gravação no vault.
---

# Auditoria de segurança do Orion

Leia antes: `Memorias Do Projeto/REGRAS.md` (núcleo imutável) e a seção de segurança do `ORION.md`. Registre cada achado com arquivo, linha, cenário concreto e correção mínima; descarte o que não tem caminho real de ataque.

## 1. OWASP Top 10 para aplicações com LLM (edição 2025)
Para cada item, procure no diff:
- **LLM01 Injeção de prompt:** texto de página, e-mail, nota, resultado de ferramenta ou MCP chegando ao prompt como se fosse instrução. Conteúdo externo é dado, não comando (REGRAS).
- **LLM02 Vazamento de informação sensível:** segredo, chave ou dado pessoal em log, audit, prompt, nota ou resposta. Confira a máscara de PII.
- **LLM03 Cadeia de suprimentos:** dependência nova, plugin, skill ou MCP sem hash/fixação; modelos baixados sem SHA-256.
- **LLM04 Envenenamento de dados/modelo:** nota ou memória gravada sem origem, fato corrigido sem registro.
- **LLM05 Saída tratada sem cuidado:** saída do modelo executada, renderizada como HTML/imagem externa ou passada a shell sem validação.
- **LLM06 Agência excessiva:** ferramenta com classe de risco menor que o efeito real; aprovação pulada; modo autônomo sem senha.
- **LLM07 Vazamento do prompt de sistema:** persona e regras expostas por rota ou log.
- **LLM08 Vetores e embeddings:** índice que mistura contextos ou vaza notas fora do escopo.
- **LLM09 Desinformação:** resposta factual sem fonte onde a regra exige citação.
- **LLM10 Consumo ilimitado:** laço de agente, download ou chamada de provedor sem teto de tempo, tokens, cota ou orçamento.

## 2. OWASP Top 10 para aplicações agênticas (edição 2026, ASI01–ASI10)
Fonte: [OWASP GenAI Security Project](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/) (nomes conferidos em resumo de terceiros, [Giskard](https://www.giskard.ai/knowledge/owasp-top-10-for-agentic-application-2026)). Para cada item, procure no diff:
- **ASI01 Agent Goal Hijack:** objetivo do agente redirecionado por documento, e-mail, página ou dado externo.
- **ASI02 Tool Misuse and Exploitation:** ferramenta legítima usada de forma indevida por instrução ambígua ou privilégio excessivo.
- **ASI03 Identity and Privilege Abuse:** agente sem identidade própria; permissões herdadas ou delegadas usadas além do devido (tokens, cofre).
- **ASI04 Agentic Supply Chain Vulnerabilities:** ferramentas, plugins, skills, MCPs ou dados carregados em tempo de execução, comprometidos.
- **ASI05 Unexpected Code Execution:** código gerado e executado (shell, scripts, skills) que um atacante consegue explorar.
- **ASI06 Memory & Context Poisoning:** memória de longo prazo, vault ou RAG corrompidos que enviesam decisões futuras.
- **ASI07 Insecure Inter-Agent Communication:** mensagens entre agentes, subagentes, Telegram ou MCP forjadas, repetidas ou interceptadas.
- **ASI08 Cascading Failures:** falha em um agente ou rotina se propagando (laços, filas, rotinas em cadeia).
- **ASI09 Human-Agent Trust Exploitation:** cartão de aprovação enganoso que leva o usuário a aprovar um erro.
- **ASI10 Rogue Agents:** agente fora da função prevista (modo autônomo, automelhoria) agindo como ameaça interna.

## 3. Específicos do Orion
- Caminhos: `PathGuard`, symlinks e junctions; nada sai das pastas seguras.
- Shell: lista de comandos e classe de risco; nenhum comando vindo de conteúdo externo.
- Cofre: segredos só no Credential Manager; nunca em `.env` commitado.
- Rede: hosts permitidos; callbacks só em loopback.
- Vault: gravação sem sobrescrita, escaneamento de segredos antes do commit.
- Telegram: `default-deny`, 2FA por código, cartões ligados à sessão.

## 4. Ferramentas (rodar à mão; o CI está pausado)
- Dependências: `cargo deny check` e `cargo audit` em `Orion_Rust/`.
- Código: Semgrep com regras de Rust e de segredos; veja as skills da Trail of Bits (link nas referências) para cadeia de dependências e revisão de design.

## Saída
Lista ordenada por severidade (crítico, alto, médio, baixo), cada item com cenário e correção; no fim, o que não foi verificado.
