use regex::Regex;
use std::sync::LazyLock;

/// Erro disparado quando um segredo ou PII proibido é encontrado em texto destinado ao Vault.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Segredo do tipo '{0}' detectado no conteúdo antes do commit (R06/R07)")]
    SecretFound(String),
}

const CRED: &str = "Credencial atribuída";

/// Palavras que, sozinhas, formam um valor de exemplo (`YOUR_API_KEY`, `change_me`, `seu-token-aqui`).
const PALAVRAS_EXEMPLO: &[&str] = &[
    "your", "seu", "sua", "my", "meu", "minha", "api", "key", "chave", "token", "secret", "segredo", "password", "senha", "pass", "example", "exemplo",
    "changeme", "change", "me", "placeholder", "redacted", "dummy", "insira", "coloque", "aqui", "here", "value", "valor", "access", "refresh", "client", "id", "bearer",
];

/// Valor de exemplo, não segredo. Vale só o valor INTEIRO: nome de variável de ambiente (`API_KEY`), repetição do próprio
/// nome (`password=password`), modelo completo (`${TOKEN}`, `{{ chave }}`) ou um valor feito só de palavras de exemplo
/// (`YOUR_API_KEY`, `change_me`, `exemplo123`) ou de marcadores (`xxxxxxxx`). Uma palavra de exemplo no meio de um
/// valor com cara de credencial (`A9-your-B7-strong`) não o isenta.
fn placeholder(chave: &str, valor: &str) -> bool {
    let v = valor.trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ';');
    let norm = |t: &str| t.to_lowercase().replace('-', "_");
    if norm(v) == norm(chave) {
        return true;
    }
    if v.contains("${") || v.contains("$(") || v.contains("{{") || v.contains("%s") || v.starts_with('$') {
        return true;
    }
    if !v.is_empty() && v.chars().all(|c| matches!(c, 'x' | 'X' | '*' | '.' | '-' | '_' | '#')) {
        return true;
    }
    let palavras: Vec<&str> = v.split(['_', '-', '.', ' ']).filter(|p| !p.is_empty()).collect();
    let so_exemplo = !palavras.is_empty()
        && palavras.iter().all(|p| {
            let base = p.trim_end_matches(|c: char| c.is_ascii_digit());
            p.len() - base.len() <= 3 && PALAVRAS_EXEMPLO.contains(&base.to_lowercase().as_str())
        });
    // nome de variável de ambiente: só maiúsculas, dígitos e `_`, com pelo menos um `_` e poucos dígitos
    let env = v.contains('_') && v.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && v.chars().filter(|c| c.is_ascii_digit()).count() <= 2;
    so_exemplo || env
}

static PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        // Chaves OpenAI / Generic AI
        ("OpenAI / API Key", Regex::new(r"(?i)\bsk-[a-zA-Z0-9_-]{20,}\b").unwrap()),
        // Google / Gemini API Keys (AIzaSy...)
        ("Google API Key", Regex::new(r"\bAIzaSy[a-zA-Z0-9_-]{20,}\b").unwrap()),
        // GitHub Tokens (ghp_, gho_, ghu_, ghs_, ghr_)
        ("GitHub Personal Access Token", Regex::new(r"\bgh[pousr]_[a-zA-Z0-9]{36,}\b").unwrap()),
        ("GitHub fine-grained token", Regex::new(r"\bgithub_pat_[a-zA-Z0-9_]{20,}\b").unwrap()),
        ("Groq API Key", Regex::new(r"\bgsk_[a-zA-Z0-9]{20,}\b").unwrap()),
        ("NVIDIA API Key", Regex::new(r"\bnvapi-[a-zA-Z0-9_-]{20,}\b").unwrap()),
        ("Google OAuth secret", Regex::new(r"\bGOCSPX-[a-zA-Z0-9_-]{16,}\b").unwrap()),
        ("Google refresh token", Regex::new(r"\b1//[a-zA-Z0-9_-]{20,}").unwrap()),
        (CRED, Regex::new(r#"(?im)\b(?P<k>[a-z0-9_]*(?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret)|password|senha|authorization)\s*[=:]\s*["']?(?P<v>[^\s"'<>`]{8,})"#).unwrap()),
        // Tokens de Bot do Telegram (123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ)
        ("Telegram Bot Token", Regex::new(r"\b[0-9]{8,10}:[a-zA-Z0-9_-]{35}\b").unwrap()),
        // Anthropic API Keys (sk-ant-...)
        ("Anthropic API Key", Regex::new(r"\bsk-ant-[a-zA-Z0-9_-]{20,}\b").unwrap()),
        // AWS Keys
        ("AWS Access Key", Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap()),
        // JWT Tokens
        ("JSON Web Token (JWT)", Regex::new(r"\beyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b").unwrap()),
        // Private Keys Header
        ("Chave Privada PEM", Regex::new(r"-----BEGIN (RSA |EC |OPENSSH |PGP |)PRIVATE KEY-----").unwrap()),
    ]
});

/// PII brasileira (spike F00.27): só com dígito verificador ou rótulo, para não marcar IDs de PDF, UUIDs de conversa etc.
struct PiiRule {
    name: &'static str,
    re: LazyLock<Regex>,
    check: Option<fn(&str) -> bool>,
}

fn digits(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

fn cpf_valid(s: &str) -> bool {
    let d = digits(s);
    if d.len() != 11 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    (9..11).all(|n| {
        let sum: u32 = d[..n]
            .iter()
            .enumerate()
            .map(|(i, x)| x * (n as u32 + 1 - i as u32))
            .sum();
        (sum * 10 % 11) % 10 == d[n]
    })
}

fn luhn_ok(d: &[u32]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, x)| {
            if i % 2 == 1 {
                let y = x * 2;
                if y > 9 {
                    y - 9
                } else {
                    y
                }
            } else {
                *x
            }
        })
        .sum();
    sum % 10 == 0
}

/// O número do cartão é o começo da sequência de dígitos que passa no Luhn (13 a 19 dígitos): o que vem depois
/// (validade, CVV) cola nos dígitos do número e não pode impedir a detecção.
fn card_valid(s: &str) -> bool {
    let d = digits(s);
    (13..=19).any(|n| n <= d.len() && luhn_ok(&d[..n]))
}

// CNPJ fica de fora de propósito: é dado público de empresa (o vault real só o tinha como exemplo em nota técnica).
static PII_RULES: [PiiRule; 5] = [
    PiiRule {
        name: "CPF",
        re: LazyLock::new(|| Regex::new(r"\b\d{3}\.?\d{3}\.?\d{3}-?\d{2}\b").unwrap()),
        check: Some(cpf_valid),
    },
    PiiRule {
        name: "RG",
        re: LazyLock::new(|| {
            Regex::new(r"(?i)\brg\b[^\n\d]{0,12}\d{1,2}\.?\d{3}\.?\d{3}(?:-?[0-9xX])?").unwrap()
        }),
        check: None,
    },
    PiiRule {
        name: "Agência/conta bancária",
        re: LazyLock::new(|| {
            Regex::new(
                r"(?i)\b(?:ag(?:[eê]ncia)?\.?\s*:?\s*\d{3,5}(?:-?[0-9x])?|(?:conta(?: corrente)?|c/c)\s*:?\s*\d{4,12}(?:-?[0-9x])?)\b",
            )
            .unwrap()
        }),
        check: None,
    },
    PiiRule {
        name: "Chave PIX",
        re: LazyLock::new(|| Regex::new(r"(?i)\bchave\s+pix\b[^\n]{0,20}[:=]\s*(?:\+?\(?\d[\d()\s.-]{6,}\d|\S{5,})").unwrap()),
        check: None,
    },
    PiiRule {
        name: "Cartão de crédito",
        re: LazyLock::new(|| {
            Regex::new(r"(?i)\b(?:cart[aã]o|card)\b[^\n\d]{0,20}((?:\d[ -]?){13,19})\b").unwrap()
        }),
        check: Some(card_valid),
    },
];

fn pii_match(text: &str) -> Option<&'static str> {
    PII_RULES.iter().find_map(|r| {
        r.re.find_iter(text)
            .any(|m| r.check.is_none_or(|c| c(m.as_str())))
            .then_some(r.name)
    })
}

/// Scanner de segredos e credenciais para o Vault (F06.13 / R06).
pub struct SecretScanner;

impl SecretScanner {
    /// Verifica se o texto contém algum segredo que viola as regras R06 / R07.
    /// Retorna `Err(ScanError)` se encontrar, ou `Ok(())` se limpo.
    pub fn scan(text: &str) -> Result<(), ScanError> {
        for (name, re) in PATTERNS.iter() {
            let achou = if *name == CRED { re.captures_iter(text).any(|c| !placeholder(&c["k"], &c["v"])) } else { re.is_match(text) };
            if achou {
                return Err(ScanError::SecretFound(name.to_string()));
            }
        }
        if let Some(name) = pii_match(text) {
            return Err(ScanError::SecretFound(name.to_string()));
        }
        Ok(())
    }

    /// Redige tokens conhecidos caso seja necessário sanitizar antes de salvar.
    pub fn redact(text: &str) -> String {
        let mut result = text.to_string();
        for (name, re) in PATTERNS.iter() {
            result = if *name == CRED {
                re.replace_all(&result, |c: &regex::Captures| if placeholder(&c["k"], &c["v"]) { c[0].to_string() } else { "[REDACTED_SECRET]".to_string() }).to_string()
            } else {
                re.replace_all(&result, "[REDACTED_SECRET]").to_string()
            };
        }
        for r in PII_RULES.iter() {
            result =
                r.re.replace_all(&result, |c: &regex::Captures| {
                    if r.check.is_none_or(|f| f(&c[0])) {
                        "[REDACTED_PII]".to_string()
                    } else {
                        c[0].to_string()
                    }
                })
                .to_string();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_scanner_detects_keys() {
        assert!(SecretScanner::scan("Minha chave é AIzaSyB2test12345678901234567890123").is_err());
        assert!(
            SecretScanner::scan("Token do bot: 123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ123456789")
                .is_err()
        );
        assert!(SecretScanner::scan("Texto normal de documentação sobre Rust e Orion").is_ok());
    }

    #[test]
    fn test_secret_redact() {
        let dirty = "A chave ghp_123456789012345678901234567890123456 foi vazada";
        let clean = SecretScanner::redact(dirty);
        assert!(!clean.contains("ghp_"));
        assert!(clean.contains("[REDACTED_SECRET]"));
    }
}
