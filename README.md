# Orion

Assistente pessoal com identidade masculina: técnico, direto, não-servil. Não é um chatbot. É uma extensão do dia a dia do Antônio: lembra do que importa, age no computador dele e responde de qualquer lugar.

*"Não é uma IA. É uma extensão do sistema nervoso."*

> **Licença:** proprietário, todos os direitos reservados — ver [LICENSE](LICENSE). Público como portfólio, não como software livre.

## Estado

O Orion roda em Python (`orion/`), num processo local. A migração para Rust (Windows primeiro, depois macOS) está decidida e planejada, sem código ainda. O estado completo, o plano e as pendências estão em [ORION.md](Memorias%20Do%20Projeto/ORION.md).

## Documentação

| Arquivo | Conteúdo |
|---|---|
| [ORION.md](Memorias%20Do%20Projeto/ORION.md) | Visão, estado por etapa, pendências suas, plano Rust, backlog e decisões |
| [REGRAS.md](Memorias%20Do%20Projeto/REGRAS.md) | Princípios do projeto, onde o código os impõe e o teste que os prova |
| [ARQUITETURA.md](Memorias%20Do%20Projeto/ARQUITETURA.md) | Módulos, dados, saídas para fora, fluxos e referências |
| [ORION_LEGADO.md](Memorias%20Do%20Projeto/ORION_LEGADO.md) | Operação do Python (ligar, variáveis, opt-ins), o legado e o histórico, num arquivo só |
| [AGENTS.md](AGENTS.md) | Instruções para agentes de IA que trabalham no repositório |

## Como rodar

Requisitos: Python 3.12 gerenciado pelo [uv](https://docs.astral.sh/uv/), e Windows 10/11 ou macOS.

```
uv sync
uv run orion chave gemini          # guarda a chave do provedor no cofre do SO (ver ORION_PROVEDORES)
uv run orion set-password          # senha do login; toda rota da API exige login
uv run orion                       # sobe em 127.0.0.1:8000
```

Extras opcionais, cada um opt-in: `uv sync --extra camera` (câmera e visão, E11), `--extra wake` (palavra de ativação), `--extra ponte` (ponte de desktop), `--extra rerank` (reranker local) e `--extra pdf`.

Windows sem instalar nada: na aba Actions, o workflow "Orion.exe (Windows)" gera o `orion.exe` (ver a seção de operação em [ORION_LEGADO.md](Memorias%20Do%20Projeto/ORION_LEGADO.md)).

Comandos úteis:

| Comando | Para quê |
|---|---|
| `uv run orion mcp-check` | Sobe os servidores do `mcp.json` e lista as ferramentas e as classes de risco |
| `uv run orion backup` / `restore <arquivo>` | Backup diário da memória (mantém 7) e restauração, com confirmação |
| `uv run orion autostart [--install]` | Mostra (ou grava) o arquivo de início automático do seu sistema |
| `uv run orion panico` | Corta captura, palavra de ativação, jobs de rede e ferramentas de rede e execução |
| `uv run orion transcrever <arquivo ou link>` | Transcreve áudio ou vídeo, com aviso antes de enviar ao provedor |
| `uv run orion verify-export <pasta>` | Prova o export da base antiga, sem tocar no banco real |

## Configuração

O arquivo de exemplo é `.env.example`. As principais variáveis:

| Variável | Uso |
|---|---|
| `ORION_ADMIN_TOKEN` | Credencial de máquina (16+ caracteres) para scripts e a ponte de desktop |
| `ORION_PROVEDORES` | Provedores de modelo em ordem de prioridade (Gemini, Groq, Cerebras, OpenRouter, Mistral, GitHub Models, NVIDIA, Z.ai ou outro com `url`) |
| `ORION_GATEWAY_URL`, `ORION_GATEWAY_MODEL`, `ORION_GATEWAY_API_KEY` | Endpoint avulso compatível com a API da OpenAI (opcional) |
| `ORION_DATA_DIR` | Onde fica o `orion.db` (padrão: pasta de dados do usuário no sistema) |
| `ORION_HOST`, `ORION_PORT`, `ORION_ALLOWED_HOSTS` | Só `127.0.0.1` por padrão. Acesso de fora (ex.: Tailscale) só sobe com login |
| `ORION_EMBED_API_KEY`, `ORION_EMBED_MODEL`, `ORION_EMBED_DIM` | Embeddings pela API gratuita do Gemini. Sem chave, a busca é só por palavra-chave |
| `ORION_TELEGRAM_TOKEN`, `ORION_TELEGRAM_ALLOWED_USERS` | Canal do celular. Sem lista de IDs, o canal não sobe |
| `ORION_TRANSCRIBE_API_KEY` | Voz (Whisper no Groq) |
| `ORION_DESKTOP_TOOLS`, `ORION_WEB_TOOLS` | Opt-ins de ferramentas de computador e de web. Padrão: desligados |
| `ORION_MCP_CONFIG`, `ORION_MCP_ENABLED` | `mcp.json` com os servidores MCP (exemplo em `mcp.example.json`) |
| `ORION_VAULT_DIR` | Vault do Obsidian, reindexado de hora em hora (vazio: não indexa) |
| `ORION_JOBS_ENABLED`, `ORION_CONSOLIDATE` | Jobs em segundo plano (lembretes, backup, consolidação) |

A lista completa de opt-ins, com o que sai do computador em cada um, está na tabela-resumo do [ORION_LEGADO.md](Memorias%20Do%20Projeto/ORION_LEGADO.md) (§16).

## Testes

```
uv sync
uv run ruff check .
uv run ruff format --check orion tests
uv run pyright
uv run pytest -q
node --test "tests/front/*.test.js"          # markdown, XSS e lógica do front

# testes de navegador (Playwright + axe-core; backend de mentira incluso)
uv sync --group e2e && (cd tests/front_e2e && npm ci)
uv run playwright install chromium
uv run pytest tests/front_e2e -q
```

O CI (`.github/workflows/ci.yml`) roda isso no GitHub Actions. Para mergear, precisa estar verde em Windows e macOS.

## Layout

```
orion/                     # o app em Python (ver ARQUITETURA.md, seção 1)
tests/                     # pytest, Node (front) e navegador (front_e2e)
Memorias Do Projeto/       # documentação: ORION, REGRAS, ARQUITETURA e ORION_LEGADO
Orion_Ollama/              # legado: backend FastAPI antigo (sai na fase 7)
Orion_Core/                # legado: front v1 e voz antiga (sai na fase 7)
bin/                       # legado: .bat dos serviços antigos
.github/workflows/         # CI e build do executável
pyproject.toml · uv.lock   # dependências do app
mcp.example.json · .env.example
```

## Segurança

- Tudo escuta em `127.0.0.1`. Acesso de fora só com login, pela rede privada do Tailscale.
- Toda rota da API exige login: senha, ou o token de admin para máquinas.
- As ferramentas seguem a política de risco. Leitura roda direto; escrita fica no audit; execução e ação destrutiva pedem aprovação no próprio canal, com botão. Voz e frase nunca aprovam.
- Os princípios e o teste que prova cada um estão em [REGRAS.md](Memorias%20Do%20Projeto/REGRAS.md).
- O legado (`Orion_Ollama/`) não tem login e não deve ser exposto na rede. Ele sai na fase 7.
- `.env`, chaves de API e credenciais OAuth nunca são versionados.

**Reportar vulnerabilidade:** não abra Issue pública. Mande um e-mail para antonio.assuino.salomao@gmail.com com os passos para reproduzir, o impacto e o commit afetado. Sem SLA formal.

### Controle Rust no Windows (PR #30)

As opções específicas do Rust estão em `Orion_Rust/.env.example`; configure-as no `.env` lido pelo executável. Na pasta `Orion_Rust/`, use `cargo run -p orion-server -- desktop` (Rust 1.91 ou superior). O controle abre o login, mantém o servidor na bandeja e oferece mini barra (`Ctrl+Alt+Espaço`), pânico (`Ctrl+Alt+P`), orçamento e rotinas. Fechar a janela mantém a bandeja; **Sair do Orion** encerra. Início com Windows é opt-in no painel ou `orion autostart --install`; a retomada do pânico é sempre explícita.

Para 2FA, configure o bot e seus IDs permitidos no cofre/ambiente e `ORION_TELEGRAM_2FA=true`. Após a senha, digite na janela o código recebido no Telegram. Sem Telegram disponível o login com 2FA fica bloqueado; a reserva por Windows Hello ainda não está implementada. `ORION_TELEGRAM_VOICE_REPLY=true` permite resposta falada aos pedidos por áudio.

Voz começa desligada. Configure `ORION_LOCAL_STT_MODEL` com um modelo Whisper ggml e `ORION_VOICE_ENABLED=true`. Para Vulkan, instale o SDK no ambiente de compilação, compile com `cargo build -p orion-server --features local-vulkan` e use `ORION_LOCAL_STT_GPU=true`; CPU é o padrão. Fallback Groq exige `ORION_TRANSCRIBE_GROQ_FALLBACK=true` além da chave. Áudio comprimido do Telegram usa ffmpeg local. TTS requer edge-tts e um reprodutor compatível no PATH.

Para escuta local, habilite `ORION_WAKE_ENABLED=true`, `ORION_WAKE_ENGINE=openwakeword` e `ORION_WAKE_MODEL` com o ONNX treinado para **Orion**. Para palmas, `ORION_CLAP_ENABLED=true`: mantenha silêncio nos primeiros dois segundos para a calibração. Palmas só abrem a janela. Respeitam pausa, pânico, não perturbe e teto por hora.

O orçamento é uma **estimativa em chamadas**, com limites configurados por provedor no painel. Não representa o uso oficial das assinaturas. Configure a cota semanal, o reset UTC e, se necessário, a janela de cinco horas. Há reserva de 5% e teto de 20% por tarefa; reiniciar o app não zera o gasto. `ORION_WEEKLY_BUDGETS` também aceita um objeto por ID, por exemplo `{"cli:codex":{"weekly_calls":100,"reset_at":1792368000,"five_hour_calls":20}}`. Os IDs das APIs são `gateway:<id do provedor>`; os das CLIs são `cli:claude`, `cli:codex` e `cli:gemini`. Cotas sem configuração não têm saldo oficial presumido.
