# Orion (Rust)

Workspace Cargo do Orion: app iced, núcleo, servidor, instalador, atualizador, voz e worker de modelos locais. Visão geral do projeto: [README da raiz](../README.md); documentação: [docs/](../docs/README.md).

## Requisitos (Windows 10/11 x64)

| O quê | Para quê |
|---|---|
| Rust 1.97.0 (o `rust-toolchain.toml` instala sozinho via `rustup`) | tudo |
| Visual Studio Build Tools 2022 (carga "Desenvolvimento para desktop com C++") | linkagem MSVC |
| LLVM (`LIBCLANG_PATH=C:\Program Files\LLVM\bin`) | `bindgen` (llama.cpp, Whisper) |
| NASM e Perl | OpenSSL embutido do SQLCipher |
| Vulkan SDK (com `glslc`) e Ninja, e `CMAKE_GENERATOR=Ninja` | só para o pacote Vulkan (`--features orion-llama/vulkan`) |

Dica: use um alvo curto (`set CARGO_TARGET_DIR=C:\t`) para evitar caminhos longos no MSBuild.

## Compilar e rodar

```
cd Orion_Rust
cargo app                         # app iced (orion-app)
cargo build -p orion-server -p orion-llama                  # orion.exe + orion-llama.exe (CPU)
cargo build -p orion-server -p orion-setup -p orion-llama \
  --features orion-server/local-vulkan,orion-server/sqlcipher,orion-llama/vulkan   # pacote Vulkan
cargo snap snaps                  # fotos das telas, sem janela
cargo dev                         # app com painel de depuração e hot reload (docs/FERRAMENTAS-DEV.md)
cargo verificar                   # cargo check --locked de todo o workspace
```

Atalhos definidos em `.cargo/config.toml`. Os executáveis ficam em `target/debug` (ou no `CARGO_TARGET_DIR`).

## Testar

Os testes continuam no código (`cargo test --locked -p <crate>`), mas **testes novos e CI estão pausados desde 21/10/2026** (ver [ADR 0007](../docs/decisoes/0007-testes-e-ci-pausados.md)). Os testes de `orion-vault` falham no Windows hoje.

## Dependências

`cargo deny check` confere licenças, avisos e origens (`deny.toml`). O Dependabot abre um PR semanal de atualização.
