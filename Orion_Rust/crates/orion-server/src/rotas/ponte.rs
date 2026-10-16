//! Escopo do token da ponte de desktop (REGRAS.md: ponte de desktop só abre e cola o que você
//! aprovou): só estas rotas; o resto responde 403.

use regex::Regex;
use std::sync::LazyLock;

pub const ESCOPO: &str = "ponte";
pub const ROTAS_DA_PONTE: [&str; 5] = ["/captura", "/ponte/imagem", "/ponte/explicar", "/posso-desligar", "/area-transferencia"];
static ROTINA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^/rotinas/[a-z0-9][a-z0-9_-]{0,31}/executar$").unwrap());

pub fn rota_permitida(metodo: &str, caminho: &str) -> bool {
    // cortar é sempre seguro; sair exige a senha
    if metodo == "POST" && caminho == "/modo/panico" {
        return true;
    }
    // tecla de rotina: só concedidas
    if metodo == "POST" && ROTINA.is_match(caminho) {
        return true;
    }
    ROTAS_DA_PONTE.iter().any(|r| caminho == *r || caminho.starts_with(&format!("{r}/")))
}
