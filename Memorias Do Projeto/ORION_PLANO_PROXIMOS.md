# ORION — Plano de próximos passos (08/10/2026)

> Junta três fontes: o plano C05–C50 de [ORION_EXTENSOES.md](ORION_EXTENSOES.md), o que a Lyra tinha e não veio, e ideias novas.
> Levantamento por leitura de docs e do vault; nada aqui foi testado. Cada item novo precisa de regra em [ORION_REGRAS.md](ORION_REGRAS.md) antes do código.
> Nota: o cliente MCP já existe (L4, `orion/mcp_client.py`); os itens C07–C14 devem ser reavaliados contra ele antes de executar, para não refazer.

## Critério de ordem
1. Dá para testar sem serviço real (Linux, API falsa) vem antes.
2. Não abre canal novo de saída de dados nem captura contínua sem aval (Ring 0 #3, regras 18, 32, 38).
3. Depende só do Orion vem antes do que depende de conta ou chave do Antônio.

## Bloco A — App confiável e conversas (C05–C06, C36–C37)
| ID | Entrega | Origem | Depende de |
|---|---|---|---|
| A1 | Renomear, fixar e arquivar conversas, foco preservado (C05) | EXTENSOES | — | **backend feito (08/10)**: renomear/fixar já existiam; `arquivada` no `PATCH /sessoes/{id}`, `GET /sessoes?arquivadas=true`, esquema v6; menu "Arquivar" e busca no conteúdo já estão no front (testados só com os testes Node e o backend de mentira) |
| A2 | Busca de conversas por conteúdo no backend (C06) | EXTENSOES | A1 | **backend feito (08/10)**: `GET /sessoes/busca?q=`, título + corpo, trecho, isolada por canal; falta ligar no front |
| A3 | Memória pesquisável: fonte, data, editar, esquecer (C36) | EXTENSOES | — | **feito (08/10)**: `GET/PATCH/DELETE /memoria/fatos` e a tela Conhecimento (`#/conhecimento`) para buscar, corrigir e esquecer |
| A4 | `orion esquecer <trecho ou id>`: apaga o fato, o índice de busca e o vetor | nova | A3 | **feito (08/10)**; avisa que backups, mensagens de conversa e notas do vault não são tocados |
| A5 | Caixa de atividade: avisos e pendências juntos (C37) | EXTENSOES | — | **feito (08/10)**: `GET /atividade` e card Atividade no Painel (avisos novos/lidos, marcar como lido). Marcar como lido na interface tira o aviso da fila do Telegram |

## Bloco B — Skills antes de plugins (C15–C18)
| ID | Entrega | Origem | Depende de |
|---|---|---|---|
| B1 | Validar e listar pacotes `SKILL.md` (C15) | EXTENSOES | — | **feito (08/10)**: `orion/skills.py`, `orion skills` |
| B2 | Carregar contexto gradual por relevância (C16) | EXTENSOES | B1 | **feito (08/10)**: só nome+descrição no prompt, corpo por `carregar_skill` |
| B3 | Invocar skills pelo chat e pela paleta (C17) | EXTENSOES | B2 |
| B4 | Confiança e scripts de skill sempre com aprovação (C18) | EXTENSOES | B3 | decisão provisória: skill **não executa script nenhum** (`scripts/` é ignorada e avisada); reavaliar se um dia precisar |

## Bloco C — Projetos e resultados (C31–C35, C38)
| ID | Entrega | Origem | Depende de |
|---|---|---|---|
| C1 | Projetos com conversas associadas (C31) | EXTENSOES | A1 | **feito (08/10)**: esquema v7, `/projetos`, `projeto_id` na conversa, instruções entram no prompt; tela Conhecimento cria, edita instruções, arquiva e apaga projetos. Falta mover conversa para projeto pela barra lateral |
| C2 | Memória, fontes e extensões isoladas por projeto (C32) | EXTENSOES | C1 |
| C3 | Biblioteca de resultados com origem, versões e prévia lateral (C34–C35) | EXTENSOES | C1 | **feito (08/10)**: esquema v10, o agente copia o que `gerar_documento`/`gerar_imagem` produziram para `<dados>/resultados`, versões por nome, `/resultados` (lista, prévia de texto/imagem, download como anexo, apagar) e card Resultados; prévia fica no próprio card, não é painel lateral |
| C4 | Ingerir PDF, texto e Markdown com progresso (C38) | EXTENSOES | C2 | **feito (08/10)**: `POST/GET/DELETE /memoria/documentos` (PDF, Word, Excel, HTML, texto, CSV, JSON; esquema v9) e card Documentos na tela Conhecimento; documento de projeto só entra no contexto automático do projeto (C32); sem barra de progresso (a indexação é uma chamada só); PDF escaneado não tem OCR |

## Bloco D — O que a Lyra tinha e não veio
| ID | Entrega | Como encaixa no Orion | Risco / regra nova |
|---|---|---|---|
| D1 | **Ciclo de sono** (dedup, cruzamento pelo grafo, destilação) — **feito (08/10)**, regra 42: `orion/memory/sleep.py` avisa duplicados, grava relações no grafo e até 3 padrões; testado só com modelo falso, desligado por padrão | Estende `consolidate.py` e o job existente; roda de madrugada, sem ferramentas | Qualidade dos fatos exige revisão humana (`facts_markdown`) |
| D2 | **Pesquisa noturna supervisionada** (ideia nº 1 do catálogo) — **feito (08/10)**, `orion/research.py`, regra 39, desligada por padrão; nunca rodou com modelo e rede reais | Job que só lê (`pesquisar_internet`, `buscar_url`) e deixa um relatório na caixa de entrada do vault; nunca executa nem escreve fora do vault | Choca com a regra de egress (V9): rodar em sessão limpa, só leitura, URL só de resultado de busca; regra nova + opt-in |
| D3 | **Screenpipe / memória da tela** — **feito (08/10)**, aval dado; regra 44, desligado por padrão; nunca rodou com tesseract nem tela reais | Captura periódica da tela, OCR local, texto no SQLite com retenção curta; imagem nunca sai | Ring 0 #3 (captura contínua) pede aval; opt-in, pausa, retenção e audit sem conteúdo |
| D4 | **Transcrição de lives** (yt-dlp + faster-whisper) — **parcial (08/10)**, decisão: Groq. `orion transcrever <arquivo>` (regra 43) funciona com arquivo local e com link (yt-dlp, só áudio, link digitado por você e validado pela barreira de rede). Groq e yt-dlp nunca foram chamados de verdade | Ferramenta que transcreve vídeo/áudio para Markdown timestampado e grava na caixa de entrada do vault | Download é egress com destino escolhido: aprovação a cada uso; whisper local pesa em 8 GB, ver D4b |
| D4b | Alternativa leve: Whisper pelo Groq (já usado no Telegram) | Reaproveita `orion/transcribe.py` | Áudio vai ao provedor; avisar |
| D5 | **n8n** (conversa de 18/08 no vault) — **feito (08/10)**, regra 41; nunca falou com um n8n real | Orion chama workflows por webhook fixo, classificado como execução no `mcp.json`/ferramenta | Só webhooks cadastrados, nunca URL escolhida pelo modelo |
| D6 | **Personas dinâmicas** — **coberto (08/10)** pelas instruções de projeto (C1); não há troca de persona base | Perfis de estilo por projeto (Bloco C) sobre a persona imutável; nunca mudam regras | A persona base continua versionada e imutável |
| D7 | **Grafo 3D de memória** | Visualização opcional sobre A3 (a lista vem primeiro) | Só front; baixa prioridade |
| D8 | **MQTT / automação do quarto** | Servidor MCP de MQTT com classes de risco no `mcp.json` | Depende de hardware; só quando houver dispositivo |
| D9 | **WhatsApp** | Adiado na Lyra (passa pelos servidores da Meta); manter adiado, Telegram cobre | Decisão do Antônio |
| D10 | Ofuscação de tráfego por ruído | **Não recomendo**: gera tráfego falso e não faz sentido com nuvem por padrão | Decisão do Antônio |

## Bloco E — Ideias novas
| ID | Entrega | Observação |
|---|---|---|
| E1 | Resumo semanal da memória e do que foi feito | **feito (08/10)**: resumo determinístico na segunda com o briefing e, com `ORION_WEEKLY_AI=true`, uma leitura de até 5 linhas escrita pelo modelo a partir do resumo (uma chamada por semana; nunca falou com modelo real) |
| E2 | Modo "só leitura" por turno (nenhuma ferramenta de escrita ou execução disponível) | **feito (08/10)**: `Agent.run(..., read_only=True)`; o que pediria aprovação é negado sem deixar pedido. Base para D2 |
| E3 | Painel de custo/cota por provedor ao longo da semana | **feito (08/10)**: respostas por dia e endpoint, persistidas em contadores (`uso:<dia>:<endpoint>`), card "Uso da semana" no painel; não mede a cota real do provedor |
| E4 | `orion doctor`: confere chaves, serviços, `mcp.json` e fase 0 em um comando | **feito (08/10)**, offline; não testa rede nem a fase 0 |
| E5 | Teste de regressão de prompt injection (conjunto fixo de páginas hostis) | **feito (08/10)**: invariantes sobre todas as ferramentas + 12 comandos hostis, `tests/policy/test_injecao_invariantes.py` |

## Bloco F — Plugins e integrações (C19–C30, C39–C50)
Só depois de A, B e C. Plugins entram com concessões revogáveis (C23) e catálogo (C25–C26); o primeiro plugin é "Orion Pesquisa" (C28–C29).

### Plugins (C19–C30) — versão mínima feita em 08/10 (regra 45)
`orion/plugins.py`: pacote local com `plugin.json`, skills e servidores MCP; **instalar só valida e copia**; nada vale sem `orion plugin conceder` (hash do pacote inteiro, mudança cancela); `GET /plugins`, `POST /plugins/{nome}/conceder|revogar` e card Plugins na tela Conhecimento. Fora desta versão: instalar por upload na interface, importar pacote de distribuição do Claude Code/Codex (C21), atualização com reversão de versão (C22), catálogo/marketplace (C25), diagnóstico de conexão MCP na interface (C26), fontes e atividade das extensões no chat (C27), pacote "Orion Pesquisa" (C28–C29).

### MCP (C07–C14) reavaliado em 08/10 contra o cliente que já existia
| Item | Situação |
|---|---|
| C07 SDK fixado e prova de protocolo | ✅ já existia (`mcp` 2.3.0 no lock, servidor de teste real em `tests/mcp_cliente`); falta só testar versão incompatível com mensagem legível |
| C08 validar argumentos pelo JSON Schema | ✅ **feito (08/10)** para ferramentas MCP (`Tool.validar`, `jsonschema`); as nativas continuam tolerantes (aceitam "5" onde o esquema diz integer) |
| C09 servidores locais por stdio | ✅ já existia |
| C10 Streamable HTTP | ✅ **feito (08/10)**: `url` + `headers` no `mcp.json`, segredos `${NOME}` do cofre (também dentro do valor, ex. `Bearer ${TOKEN}`), testado contra servidor HTTP real do SDK; OAuth do MCP não implementado |
| C11 classes de risco e limites | ✅ já existia (regra 24) |
| C12 timeout, cancelamento, reconexão | 🟡 timeout existe; **reconexão automática não** (servidor que cai fica marcado como falho até reiniciar o Orion) |
| C13 descoberta sob demanda | ⏳ não feito; só vale a pena com catálogo grande |
| C14 resources e prompts | ⏳ não feito; o cliente só usa `tools` |

## Fora do código (só o Antônio)
Fase 0 e `verify-export`, venda do PC, serviços e chaves, teste de voz e da palavra de ativação com microfone real, `Orion-windows-x64.zip`, senha de fábrica ([ORION_OPERACAO.md](ORION_OPERACAO.md), [ORION_CORTE.md](ORION_CORTE.md)).

## Sequência sugerida
1. A1, A2, A3 (testáveis já).
2. E4, E5 (reduzem risco antes de abrir mais).
3. B1–B4.
4. C1–C4.
5. D1 e E1.
6. D2, D4 e D3, cada um com regra nova e aval.
7. Bloco F; D5–D8 conforme necessidade.

## Decisões que preciso de você
- D3 (captura de tela contínua) e D2 (pesquisa noturna): liberar o desenho?
- D4: Whisper local ou Groq?
- D9 e D10: manter adiado e descartado?
