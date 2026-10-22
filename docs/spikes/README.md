# Spikes

Cada spike mede um risco e termina com uma decisão escrita neste diretório. Os spikes de Linux/CPU foram rodados em 22/10/2026 (4 vCPU, 15 GB, sem GPU); os de hardware real, GPU, Windows ou conta externa continuam abertos. Testes e CI continuam pausados (ADR 0007): os spikes são programas medidores, não testes. O código fica em `Orion_Rust/spikes/*` (crates fora do workspace, não entram no build) e `orion-vault/examples/spike_pii.rs`. Modelos pesados nunca entram no pacote: o instalador baixa (ADR 0009).

| Spike | Item | O que falta |
|---|---|---|
| iced | F00.14 | Medir RAM, tempo até a janela e fps no Windows; decidir iced × Slint |
| wgpu | F00.15 | Widget `shader` do céu a 60 fps; medir CPU/GPU |
| grafo 3D | F00.16 | CPU rodado; falta compute shader com GPU |
| banco | F00.17 | Rodado em Linux; validar no Windows |
| pacote | F00.18 | Pacote esparso, toast Aprovar/Negar e OCR do Windows (só no Windows) |
| ACP e CLIs | F00.20 | ACP e medição de latência e confiabilidade |
| roteador | F00.21 | Medir contra 100 pedidos reais |
| áudio | F00.22 | Silero VAD, latência fim a fim e teste de eco com alto-falante |
| injection | F00.23 | Rodado (195 exemplos); falta injeção indireta com dados reais |
| embeddings | F00.24 | Rodado; decidir se vale o híbrido |
| `llama-cpp-2` | F00.25 | Vulkan e medições de RAM e troca de modelo |
| vault Git | F00.26 | `git2` rodado em Linux; falta Credential Manager no Windows |
| PII | F00.27 | Rodado; regras aplicadas no `SecretScanner` (CPF, RG, agência/conta, chave PIX e cartão rotulados; CNPJ fora por ser dado público) |

Cancelado: WebView2 (F00.19, decisão de 20/10/2026).

## Resultados de 22/10/2026 (Linux, CPU)

**F00.27 PII** (`orion-vault/examples/spike_pii.rs`, 1.064 notas, 13 MB, 4,6 s). `cloakrs` não existe no crates.io; ficaram as regras próprias. Achou 1 CPF válido e 1 RG reais numa conversa de cadastro de cliente (hoje o `SecretScanner` não os bloqueia). Falsos positivos: cartão por Luhn (3 de 3 eram IDs de PDF/artigo), UUID como chave PIX (30 de 30 eram IDs de conversa), "Credencial atribuída" (15 de 15 eram placeholders como `api_key=API_KEY`). Decisão: CPF/CNPJ com dígito verificador, RG e agência/conta só com rótulo, PIX só com rótulo, cartão só com rótulo; "Credencial atribuída" precisa ignorar valores que são nome de variável ou placeholder.

**F00.24 embeddings** (`spikes/embeddings`, e5-small int8, 300 consultas = `resumo` da nota → nota, corpus de 1.064 notas):

| | R@1 | R@5 | MRR |
|---|---|---|---|
| FTS5 (bm25) | 0,723 | 0,933 | 0,816 |
| e5-small | 0,680 | 0,887 | 0,772 |
| híbrido (RRF) | 0,787 | 0,940 | 0,858 |

Indexar 1.064 notas: FTS5 0,6 s; embeddings 70 s (15 notas/s, 4 vCPU). Consulta: 10 ms. RAM: +146 MB com o modelo (pico ~640 MB em lote). Modelo: 118 MB, baixado pelo instalador. Ganho do híbrido: +6,4 pontos em R@1; FTS5 já acerta 93% no top 5. Consistente com a decisão BM25 + LSA; embeddings só se o ganho de R@1 compensar 118 MB e 70 s de indexação. Limite: consultas vêm do `resumo`, que tem sobreposição lexical alta.

**F00.17 banco** (`spikes/banco`): SQLCipher 4.5.7 + FTS5 + sqlite-vec 0.1.9 + log de eventos no mesmo arquivo, funcionando (arquivo cifrado, senha errada recusada, UPDATE/DELETE do log bloqueados por trigger, cadeia de hash detecta adulteração). 10.000 itens × 384 dim: inserir tudo em 0,6 s; KNN k=10 em 52 ms (varredura exata); FTS5 1,6 ms; híbrido FTS5 como filtro do `vec0` 54 ms. `sqlite-vec` 0.1.10-alpha.4 não compila (falta arquivo no pacote): fixar `=0.1.9`. Falta validar no Windows.

**F00.26 vault Git** (`spikes/vault-git`): `git2` faz commit só dos caminhos pedidos (árvore montada do HEAD num índice em memória: nota staged por outro programa fica fora do commit e continua staged), push, e pull só por fast-forward com checkout seguro. Edição não commitada do Obsidian na mesma nota bloqueia o pull ("1 conflict prevents checkout") sem sobrescrever; em outra nota o pull avança e preserva a edição; push divergente é rejeitado e o pull detecta divergência (sem merge automático). Push por HTTP com `credential_helper` funcionou contra servidor com Basic Auth; credencial errada é recusada. `status` do vault real: 6 ms. Falta provar com o Git Credential Manager no Windows. `gix` não testado.

**F00.23 injection** (`spikes/injection`, `dataset.tsv` com 195 exemplos pt-BR escritos à mão: 97 injeções, 98 normais; limiar 0,5):

| Modelo | Recall | Falso positivo | Latência p50 | RAM | Disco | Licença |
|---|---|---|---|---|---|---|
| Horizon-Labs/prompt-injection-guard-small (ModernBERT int8) | 92,8% | 0,0% | 25 ms | 122 MB | 268 MB | Apache-2.0 |
| ZeroLeaks/shield-small (int8) | 90,7% | 3,1% | 9 ms | 139 MB | 118 MB | CC-BY-NC-4.0 |
| protectai/deberta-v3-base-prompt-injection-v2 | 100% | 50% | 49 ms | 1.043 MB | 738 MB | Apache-2.0 |

DeBERTa é só inglês: inutilizável em pt-BR (confirmado em inglês, onde acerta). Melhor: ModernBERT; com limiar 0,09 chega a 99% de recall e 1% de falso positivo (limiar ajustado no mesmo conjunto, então otimista; o DeBERTa não atinge FP ≤ 2% em nenhum limiar). Shield Small tem licença não comercial. Limites: exemplos curtos e escritos por mim; falta injeção indireta em documentos longos e dados reais.

**F00.16 grafo, parte CPU** (`spikes/grafo`, layout de forças 3D, arestas ≈ 3n, 4 threads):

| Nós | O(n²) paralelo | Barnes-Hut paralelo | Barnes-Hut 1 thread |
|---|---|---|---|
| 1.000 | 1,2 ms | 1,3 ms | 3,4 ms |
| 5.000 | 19,5 ms | 11,8 ms | 37,3 ms |
| 10.000 | (não rodado) | 28,7 ms | 112 ms |

Fallback na CPU é viável: 10.000 nós a ~35 iterações/s com 4 threads. O compute shader precisa de GPU e continua aberto.

## Peso do Orion e do instalador (medido em 22/10/2026)

Build release no Linux (`cargo build --release -p orion-server -p orion-llama -p orion-setup --features orion-server/sqlcipher`, ~25 min em 4 vCPU). **São artefatos Linux incompletos, não o produto:** `orion-llama` só compila inferência em Windows (no Linux é um stub que sai), e `orion-voice` exclui Whisper, openWakeWord e AEC3 fora do Windows. Logo o `orion` abaixo não tem os componentes nativos de voz e IA locais, e o Windows será maior. Não há estimativa válida do tamanho final; medir os `.exe` do release do CI.

| Arquivo | Bruto | gzip -9 | xz -6 |
|---|---|---|---|
| `orion` (app + núcleo, variante CPU) | 57,0 MB | 23,1 MB | 16,5 MB |
| `orion-llama` (worker, CPU) | 0,3 MB | — | — |
| `orion-setup` (instalador: janela, céu, fontes) | 22,4 MB | 9,3 MB | 6,5 MB |
| `wake-candidate` (palavra de ativação, `models/`) | 1,5 MB | — | — |

- **Instalador (`orion-setup`):** ~22 MB no Linux; é a janela, o céu e as fontes, sem dependência de voz/IA, então o do Windows deve ser da mesma ordem (confirmar).
- **Orion instalado:** o 57 MB do Linux é piso, não estimativa; a dica "~150 MB" da especificação do instalador continua a referência até medir o Windows.
- **Download do app pelo instalador:** o feed serve o `.exe` cru; gzip reduziria a ~40% do tamanho (23 de 57 MB no Linux).
- **Modelos (fora do pacote, baixados pelo instalador):** os do spike de injection somam 268 MB (ModernBERT) e os de embeddings 118 MB (e5-small); os 8 modelos locais da ADR 0009 vão de 0,8 a 14 GB cada.
