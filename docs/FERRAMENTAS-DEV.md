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
