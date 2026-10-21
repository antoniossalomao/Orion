use regex::Regex;
use std::sync::LazyLock;

/// Erro disparado quando um segredo ou PII proibido é encontrado em texto destinado ao Vault.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Segredo do tipo '{0}' detectado no conteúdo antes do commit (R06/R07)")]
    SecretFound(String),
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
        ("Credencial atribuída", Regex::new(r#"(?im)\b(?:[a-z0-9_]*(?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret)|password|senha|authorization)\s*[=:]\s*["']?[^\s"'<>`]{8,}"#).unwrap()),
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

/// Scanner de segredos e credenciais para o Vault (F06.13 / R06).
pub struct SecretScanner;

impl SecretScanner {
    /// Verifica se o texto contém algum segredo que viola as regras R06 / R07.
    /// Retorna `Err(ScanError)` se encontrar, ou `Ok(())` se limpo.
    pub fn scan(text: &str) -> Result<(), ScanError> {
        for (name, re) in PATTERNS.iter() {
            if re.is_match(text) {
                return Err(ScanError::SecretFound(name.to_string()));
            }
        }
        Ok(())
    }

    /// Redige tokens conhecidos caso seja necessário sanitizar antes de salvar.
    pub fn redact(text: &str) -> String {
        let mut result = text.to_string();
        for (_, re) in PATTERNS.iter() {
            result = re.replace_all(&result, "[REDACTED_SECRET]").to_string();
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
