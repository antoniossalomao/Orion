# Ferramentas de desenvolvimento do iced 0.14

A feature `dev` de `orion-app` liga `iced/debug`, `iced/time-travel` e `iced/hot`. **Só para desenvolvimento**; o pacote de release não a usa.

```
cd Orion_Rust
cargo dev
```

- **Painel de depuração (`debug`):** F12 abre o painel do iced com métricas de `update`, `view`, desenho e eventos.
- **Viagem no tempo (`time-travel`):** o painel permite recuar e reavançar pelas mensagens já processadas, reproduzindo o estado da interface. Exige `Msg: Clone`, que já vale no app.
- **Hot reload (`hot`):** `update`, `view`, `title`, `subscription`, `theme`, `style` e `scale_factor` passam por `debug::hot`. Para recarregar ao salvar, rode o app pelo `cargo-hot` do iced (`cargo install cargo-hot --locked`, depois `cargo hot -p orion-app --features dev`). Sem o `cargo-hot`, a feature não altera nada.

Não foi possível provar o hot reload e a viagem no tempo numa janela real neste ambiente; a feature compila (`cargo check -p orion-app --features dev`).

## Rodar o app no Linux, sem Windows (Xvfb + LLM de teste)

Serve para ver o app de verdade e dirigir o fluxo (onboarding, chat, aprovação, Configurações) sem uma janela de Windows. **Não substitui o teste no Windows:** Mica, bandeja, atalhos globais, Windows Hello e autostart só existem lá.

```
# dependências do sistema (Ubuntu): libasound2-dev libxkbcommon-x11-0 xvfb xdotool openbox imagemagick
python3 scripts/mock_llm.py 9099 &          # LLM falso compatível com OpenAI; MOCK_PATH=<arquivo> define o que "ESCREVER" grava
Xvfb :99 -screen 0 1440x900x24 &  DISPLAY=:99 openbox &
cd Orion_Rust && cargo build -p orion-server --features orion-server/sqlcipher
DISPLAY=:99 HOME=$(mktemp -d) ORION_GATEWAY_URL=http://127.0.0.1:9099/v1 ORION_GATEWAY_MODEL=mock ORION_DESKTOP_TOOLS=1 \
  target/debug/orion
```

- Use `xdotool` (`mousemove`, `click`, `type`) para dirigir a janela e `import -window root arquivo.png` (ImageMagick) para capturar. Sem `openbox` a janela não recebe teclado.
- O mock responde com Markdown (lista, tabela, código, citação); a frase `ESCREVER` faz o modelo pedir `escrever_arquivo` e exercita o cartão de aprovação.
- O renderizador por software deste ambiente deixa resíduo de pixels (por exemplo uma barra depois de um parágrafo): some ao redesenhar e não acontece com a GPU.
- Os PNG das telas com dados de exemplo continuam saindo de `cargo run -p orion-app --features snap --bin orion-snap -- <pasta>`.

