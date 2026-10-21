# Spikes

Cada spike mede um risco e termina com uma decisão escrita neste diretório. Nenhum foi concluído ainda: todos exigem medição em hardware real, download de modelos, GPU, conta externa ou um Windows de verdade, e testes e CI estão pausados (ADR 0007).

| Spike | Item | O que falta |
|---|---|---|
| iced | F00.14 | Medir RAM, tempo até a janela e fps no Windows; decidir iced × Slint |
| wgpu | F00.15 | Widget `shader` do céu a 60 fps; medir CPU/GPU |
| grafo 3D | F00.16 | Layout em compute shader com 1.000/5.000/10.000 nós, com GPU |
| banco | F00.17 | Acrescentar `sqlite-vec`; validar o log de eventos |
| pacote | F00.18 | Pacote esparso, toast Aprovar/Negar e OCR do Windows (só no Windows) |
| ACP e CLIs | F00.20 | ACP e medição de latência e confiabilidade |
| roteador | F00.21 | Medir contra 100 pedidos reais |
| áudio | F00.22 | Silero VAD, latência fim a fim e teste de eco com alto-falante |
| injection | F00.23 | Três modelos ONNX em português com 200 exemplos |
| embeddings | F00.24 | multilingual-e5-small no vault real (895 notas) |
| `llama-cpp-2` | F00.25 | Vulkan e medições de RAM e troca de modelo |
| vault Git | F00.26 | `gix`/`git2` com credencial do Windows e conflito com o Obsidian |
| PII | F00.27 | `cloakrs` e regras próprias em notas reais |

Cancelado: WebView2 (F00.19, decisão de 20/10/2026).
