//! Worker privado, distribuído pelo orion-setup; nunca abre porta ou herda credenciais.
//! Processo separado também evita misturar as duas versões do GGML (llama.cpp e Whisper).
#[cfg(windows)]
mod inference;

fn main() {
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        if inference::worker().is_ok() {
            return;
        }
    }
    std::process::exit(1);
}
