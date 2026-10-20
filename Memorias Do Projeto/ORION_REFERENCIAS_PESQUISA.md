---
type: recurso
projeto: "[[Orion]]"
tags: [orion, pesquisa, referencias, rust]
date: 2026-10-16
resumo: "Referências da pesquisa extensa do Orion (~40 buscas, ~350 links lidos): bibliotecas Rust, modelos locais, segurança, voz, integrações e funções novas. Decisões em [[ORION - PLANEJAMENTO ARQUITETURA]]."
---

# ORION - REFERÊNCIAS DE PESQUISA

> Projeto: [[Orion]] · Planejamento: [[ORION - PLANEJAMENTO ARQUITETURA]] · Conceito: [[Rust]]

## Interface e app
- [iced 0.14](https://github.com/iced-rs/iced/releases): renderização reativa, time-travel debug, API de animação, testes headless e E2E, hot reload. Comparações: [Phoronix](https://www.phoronix.com/news/Iced-0.14-Rust-GUI-LIbrary), [byteiota](https://byteiota.com/iced-0-14-rust-gui-gets-reactive-rendering-time-travel/), [egui × iced × slint (2026)](https://www.pistack.xyz/posts/2026-08-25-rust-gui-frameworks-egui-iced-slint-comparison/)
- [icebreaker](https://gittrend.io/repo/hecrj/icebreaker) (chat iced + llama.cpp) · [iced_term](https://gittrend.io/repo/Harzu/iced_term)
- [window-vibrancy](https://github.com/tauri-apps/window-vibrancy) (Mica só no Windows 11; blur/acrylic engasgam ao arrastar)
- Toasts: [winrt-toast](https://rust-digger.code-maven.com/crates/winrt-toast), [win-toast-notify](https://docs.rs/win-toast-notify), [winrt-notification (sem botões)](https://crates-io.ganymede-static.progval.net/crates/winrt-notification/0.5.1)
- Markdown: [scrybe-render](https://docs.rs/scrybe-render), [mordant](https://docs.rs/mordant), pulldown-cmark-katex. **Mermaid nativo não existe** → WebView sob demanda.
- Grafo: [graphpu](https://gittrend.io/repo/latentcat/graphpu) (wgpu 3D), [GraphWaGu](https://par.nsf.gov//servlets/purl/10384648) (Barnes-Hut em compute shader), [fdg](https://docs.rs/crate/fdg-sim/^0.4.0)
- Instalador: [cargo-wix](https://docs.rs/crate/cargo-wix/latest), [Velopack](https://github.com/Timskt/Velopack.UI). OCR nativo exige MSIX: [Windows.Media.Ocr](https://learn.microsoft.com/uwp/api/windows.media.ocr)

## Núcleo, dados e sync
- MCP: [rmcp](https://docs.rs/crate/rmcp/3.0.1/source/README.md) · MCP Apps: [blog MCP](https://blog.modelcontextprotocol.io/tags/apps/), [Thoughtworks Radar](https://www.thoughtworks.com/radar/platforms/mcp-apps), [WorkOS](https://workos.com/blog/2026-01-27-mcp-apps)
- ACP: [zed.dev/acp](https://zed.dev/acp), [agentes ACP](https://agentclientprotocol.com/overview/agents), [codex-acp](https://zed.dev/blog/codex-is-live-in-zed)
- CLIs: [Claude Code headless](https://docs.claude.com/en/docs/claude-code/headless), [codex exec](https://developers.openai.com/codex/noninteractive.md), [Gemini CLI headless](https://geminicli.com/docs/cli/headless.md) (aviso: free tier → Antigravity CLI)
- Provedores grátis 2026: [OpenRouter: comparativo](https://openrouter.ai/blog/tutorials/free-llm-apis-compared/), [wetheflywheel](https://wetheflywheel.com/en/ai-model-access/free-llm-api-tiers-2026/), [mudanças no Gemini](https://agentdeals.dev/gemini-api-pricing-changes)
- Roteamento: [RouteLLM](https://lmsys.org/blog/2024-07-01-routellm/), [Gradient Flow](https://gradientflow.com/llm-routers-unpacked/)
- Memória: [Mem0 × Zep × Letta](https://atlan.com/know/best-ai-agent-memory-frameworks-2026/), [dev.to](https://dev.to/agdex_ai/letta-vs-zepgraphiti-vs-mem0-choosing-an-ai-agent-memory-architecture-155a)
- Banco: [rusqlite (bundled-sqlcipher)](https://docs.rs/crate/rusqlite/*), [PBKDF2 × Argon2id no SQLCipher](https://informatika.stei.itb.ac.id/~rinaldi.munir/Kriptografi-dan-Koding/2025-2026/Makalah2026\A%20Comparative%20Analysis%20of%20PBKDF2%20and%20Argon2id%20as%20Key%20Derivation%20Functions%20in%20SQLCipher.pdf), [sqlite-vec](https://openapps.pro/packages/sqlite-vec)
- Sync: [cr-sqlite / CRDTs](https://crdt.tech/implementations), [Automerge](https://forums.swift.org/t/introducing-automerge-enable-collaborative-asynchronous-syncing-for-your-data-structures/67985)
- Git: [gitoxide/gix](https://rustutils.com/tools/gitoxide/)
- Plugins: [Wasmtime + Component Model](https://dev.to/topheman/webassembly-component-model-building-a-plugin-system-58o0)

## Modelos locais
- LLMs: [modelos pequenos 2026](https://www.labellerr.com/blog/best-small-language-models-under-10b-parameters/), [tool calling local](https://d-central.tech/local-llm-agent-capability/) (Qwen 3 8B, Gemma 4 E4B, Granite 4.1 8B, Phi-4-mini, Mistral Small 3.2)
- llama.cpp em Rust: [llama-cpp-2](https://openapps.pro/packages/llama-cpp-2), [embellama](https://docs.rs/embellama)
- Embeddings: [multilingual-e5-small](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/maas/e5/multilingual-e5-small), [versão só PT](https://huggingface.co/alphaedge-ai/multilingual-e5-small-por-32768), [bge-m3-onnx](https://github.com/yuniko-software/bge-m3-onnx)
- Visão: [Moondream 2](https://playground.roboflow.com/models/moondream-ai/moondream-2), [modelos locais de visão 2026](https://www.promptquorum.com/de/power-local-llm/local-vision-models-llava-ollama-2026) (MiniCPM-V 4.5)
- Rosto e objetos: [od_opencv](https://docs.rs/crate/od_opencv/latest) (YOLO + ArcFace via ORT), [usls](https://github.com/oatiz/usls), [rust-faces](https://docs.rs/crate/rust-faces/latest) · Câmera: [nokhwa](https://docs.rs/crate/nokhwa)

## Voz
- STT: [Whisper turbo em PT (WER ~4,2%)](https://models.handy.computer/languages/pt), [large-v3 × turbo](https://vexascribe.com/whisper-large-v3-vs-turbo)
- Wake word: [oww-rs](https://docs.rs/crate/oww-rs), [rustpotter](https://v50.openhab.org/addons/voice/rustpotterks/)
- Eco: [aec3](https://docs.rs/crate/aec3/0.4.0) · TTS: [Kokoro (tem pt-br)](https://docsearch.algolia.com/mcp/docs/repo/hexgrad/kokoro), [kokoroxide](https://docs.rs/kokoroxide)
- Full-duplex: [PersonaPlex 7B](https://rywalker.com/research/nvidia-personaplex), [DuplexCascade](https://arxiv.org/abs/2603.09180)

## Segurança
- [OWASP LLM Top 10 2026](https://www.superblocks.com/blog/owasp-llm-top-10), [OWASP Agentic Top 10](https://www.practical-devsecops.com/owasp-top-10-agentic-applications/)
- MCP: [CSA: tool poisoning e rug pull](https://labs.cloudsecurityalliance.org/research-rb/csa-whitepaper-mcp-security-tool-poisoning-20260506-csa-styl/), [MCP-Scan](https://appsecsanta.com/mcp-scan), [ataques que scanners não pegam](https://www.softwareseni.com/tool-poisoning-rug-pulls-and-the-mcp-attack-classes-conventional-scanners-miss)
- Prompt injection: [Shield Small](https://huggingface.co/ZeroLeaks/shield-small), [ProtectAI DeBERTa](https://huggingface.co/protectai/deberta-v3-small-prompt-injection-v2)
- PII: [cloakrs (CPF/CNPJ, chaves)](https://users.rust-lang.org/t/cloakrs-0-3-0-a-blazing-fast-rust-pii-scanner-and-redaction-toolkit/140244)
- Dependências: [cargo-vet](https://safeguard.sh/resources/blog/rust-supply-chain-cargo-vet-expansion-2025)
- Login: [totp-rs](https://openapps.pro/packages/totp-rs), [keyring](https://lib.rs/crates/keyring), [windows-dpapi](https://docs.rs/windows-dpapi), [Hello + Credential Manager (Go)](https://pkg.go.dev/github.com/byteness/keyring/winhello)
- Skills de auditoria: [Trail of Bits](https://mintlify.com/trailofbits/skills/introduction), [secure-design-review](https://claudskills.com/skills/secure-design-review/)

## UX de agente e qualidade
- [Padrões de UX para agentes (Hatchworks)](https://hatchworks.com/blog/ai-agents/agent-ux-patterns/), [human-in-the-loop](https://aiuxplayground.com/guides/how-to-design-human-in-the-loop), [8 padrões de confiança](https://pub.towardsai.net/designing-for-ai-agents-8-ux-patterns-that-build-trust-in-2026-713a0c926933)
- Evals: [frameworks 2026](https://futureagi.com/blog/best-open-source-eval-frameworks-2026), [promptfoo para agentes](https://dailyaiworld.com/blogs/promptfoo-agent-evaluation-2026)
- PDF: [pdf_oxide × outras libs Rust](https://pdf.oxide.fyi/docs/comparison/rust)

## Integrações
- Telegram: [teloxide](https://smithery.ai/skills/timequity/telegram-bot), [Bot API local (arquivos até 2 GB)](https://bigmike.help/en/devops/local-telegram-bot-api-advantages-limitations-of-the-standard-api-and-set-eb4a3b/), [whisper-telegram-mcp](https://glama.ai/mcp/servers/abid-mahdi/whisper-telegram-mcp)
- Alexa: [Voice Monkey](https://voicemonkey.io/), [AlexaPy](https://pypi.org/project/AlexaPy/1.29.1)
- iPhone: [Atalhos da Tailscale](https://tailscale.com/blog/ios-macos-shortcuts), [Atalhos + webhook (Home Assistant)](https://github.com/SeanPM5/homeassistant-config/wiki/Apple-Shortcuts---Home-Assistant-iOS-Tutorial)
- Wake-on-LAN: [wakey](https://lib.rs/crates/wakey), [waker](https://docs.rs/waker); precisa de um aparelho ligado na mesma rede ([r/Tailscale](https://redlib.hackliberty.org/r/Tailscale/comments/1jublor/question_wake_on_lansoftware))
- Google: [google-apis-rs (em manutenção)](https://docsearch.algolia.com/mcp/docs/repo/byron/google-apis-rs), [google-calendar](https://docs.rs/crate/google-calendar/latest)
- Spotify: [Player API](https://engineering.atspotify.com/2022/4/spotifys-player-api) (exige Premium)
- Navegador: [chromiumoxide](https://docs.rs/crate/chromiumoxide)
- Obsidian: [Local REST API](https://community.obsidian.md/plugins/obsidian-local-rest-api), URI `obsidian://`
- Finanças: [ofx-rs (BRL)](https://docs.rs/crate/ofx-rs/latest), [rustledger-importer](https://docs.rs/rustledger-importer), [actual-ai](https://github.com/sakowicz/actual-ai)
- Estudo: [fsrs](https://docs.rs/crate/fsrs/latest)
- Tela e OCR: [windows-capture](https://docs.rs/windows-capture), [screenpipe](https://screenpi.pe/about), [Recall / Click to Do](https://tomshardware.com/software/windows/microsoft-launches-recall-to-windows-11-general-availability-click-to-do-and-improved-search-also-coming)
- Controle do PC: [uiautomation-rs](https://gittrend.io/repo/leexgone/uiautomation-rs) · Tempo: [ActivityWatch](https://activitywatch.net/)
- Imagem grátis: [panorama 2026](https://blog.laozhang.ai/en/posts/free-ai-image-generation-api.md) (Gemini sem free tier estável; Pollinations)
