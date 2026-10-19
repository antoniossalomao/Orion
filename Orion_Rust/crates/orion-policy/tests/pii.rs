//! Máscara de dados pessoais em log e audit.

use orion_policy::{mask_pii, redact, redact_pii, MASCARA};
use serde_json::json;

#[test]
fn mascara_email_cpf_cnpj_telefone_e_cartao() {
    for (entrada, some) in [
        ("fale com antonio.salomao+x@gmail.com hoje", "antonio.salomao+x@gmail.com"),
        ("CPF 529.982.247-25 do cliente", "529.982.247-25"),
        ("cpf 52998224725 sem pontos", "52998224725"),
        ("CNPJ 11.222.333/0001-81", "11.222.333/0001-81"),
        ("ligue (11) 91234-5678 agora", "91234-5678"),
        ("whats +55 11 91234-5678", "91234-5678"),
        ("cartão 4539 1488 0343 6467", "4539 1488 0343 6467"),
    ] {
        let saida = mask_pii(entrada);
        assert!(!saida.contains(some), "{entrada:?} -> {saida:?}");
        assert!(saida.contains(MASCARA), "{saida:?}");
    }
}

#[test]
fn nao_mascara_numero_que_nao_e_documento() {
    for texto in [
        "pedido 12345678901 aguardando",    // 11 dígitos, mas dígito verificador de CPF inválido
        "id 1699999999 criado",             // timestamp
        "cartão 4539 1488 0343 6468",       // Luhn inválido
        "versão 0.1.0 em 2026-10-19 12:30", // datas e versões
        "arquivo orion-3.2.1.exe",
        "11111111111",                      // CPF com todos os dígitos iguais
    ] {
        assert_eq!(mask_pii(texto), texto, "{texto:?}");
    }
}

#[test]
fn redact_comum_preserva_email_mas_o_de_audit_nao() {
    let v = json!({"para": "pessoa@exemplo.com", "api_key": "abc", "aninhado": ["tel (21) 98888-7777"]});
    assert_eq!(redact(&v, 500)["para"], "pessoa@exemplo.com", "cartão de aprovação precisa mostrar o destino");
    let a = redact_pii(&v, 500);
    assert_eq!(a["para"], MASCARA);
    assert_eq!(a["api_key"], MASCARA);
    assert!(!a["aninhado"][0].as_str().unwrap().contains("98888"));
}
