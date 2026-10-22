//! Barreira de rede para URLs que vêm do modelo (REGRAS.md: URL do modelo só para host público).
//!
//! O modelo monta URLs a partir de texto não confiável. Sem esta checagem, uma injeção de prompt
//! aponta `buscar_url` para o próprio Orion (127.0.0.1), para a LAN, para a rede do Tailscale ou
//! para o endpoint de metadados de uma nuvem. O IP conferido é o IP usado: a conexão vai para o
//! IP, com o `Host` e o SNI do nome original. Cada redirecionamento passa de novo pela checagem.

use futures::StreamExt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

pub const MAX_REDIRECTS: usize = 5;
pub const MAX_BYTES: usize = 2_000_000;
pub const USER_AGENT: &str = "OrionBot/1.0 (assistente pessoal)";
const TIPOS_TEXTO: [&str; 6] = ["text/", "application/json", "application/xml", "application/xhtml", "+json", "+xml"];

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct UrlBloqueada(pub String);

fn v4_publico(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // CGNAT (Tailscale)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // atribuições do IETF
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // testes de desempenho
        || o[0] >= 240) // reservado
}

fn v6_publico(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_publico(v4);
    }
    let s = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // local único
        || (s[0] & 0xffc0) == 0xfe80 // link-local
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentação
        || (s[0] == 0x2001 && s[1] < 0x0200) // atribuições do IETF
        || (s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0) // descarte
        || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0)) // compat IPv4
}

/// `ipaddress.is_global and not is_multicast`.
pub fn ip_publico(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v4_publico(v),
        IpAddr::V6(v) => v6_publico(v),
    }
}

pub async fn resolver_dns(host: &str, porta: u16) -> Result<Vec<IpAddr>, UrlBloqueada> {
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let addrs = tokio::net::lookup_host((host, porta)).await.map_err(|_| UrlBloqueada(format!("host não resolve: {host}")))?;
    Ok(addrs.map(|a| a.ip()).collect())
}

/// URL já validada e o IP público em que vamos conectar.
#[derive(Debug, Clone)]
pub struct Destino {
    pub url: url::Url,
    pub host: String,
    pub ip: IpAddr,
    pub porta: u16,
}

/// Devolve o destino se a URL for http(s) e **todos** os IPs do host forem públicos.
pub async fn validar(u: &str) -> Result<Destino, UrlBloqueada> {
    let p = url::Url::parse(u.trim()).map_err(|e| UrlBloqueada(format!("URL inválida: {e}")))?;
    if !matches!(p.scheme(), "http" | "https") {
        let e = if p.scheme().is_empty() { "vazio".to_string() } else { format!("'{}'", p.scheme()) };
        return Err(UrlBloqueada(format!("esquema não permitido: {e} (use http/https)")));
    }
    if !p.username().is_empty() || p.password().is_some() {
        return Err(UrlBloqueada("URL com usuário/senha embutidos".into()));
    }
    let host = p.host_str().ok_or_else(|| UrlBloqueada("URL sem host".into()))?.to_string();
    let porta = p.port_or_known_default().unwrap_or(80);
    let ips = resolver_dns(&host, porta).await?;
    if ips.is_empty() {
        return Err(UrlBloqueada(format!("host não resolve: {host}")));
    }
    for ip in &ips {
        if !ip_publico(*ip) {
            return Err(UrlBloqueada(format!("destino não público bloqueado: {host} -> {ip}")));
        }
    }
    Ok(Destino { ip: ips[0], url: p, host: host.trim_matches(['[', ']']).to_string(), porta })
}

#[derive(Debug, Clone)]
pub struct Resposta {
    pub status: u16,
    pub url_final: String,
    pub tipo: String,
    pub corpo: Vec<u8>,
    pub truncado: bool,
}

/// GET seguro: valida a URL e cada redirecionamento, conecta no IP conferido, limita o tamanho.
pub async fn buscar(u: &str, timeout_s: f64, max_bytes: usize) -> anyhow::Result<Resposta> {
    let mut atual = u.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let d = validar(&atual).await?;
        // o IP conferido é o IP usado: sem janela de DNS rebinding
        let cliente = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs_f64(timeout_s))
            .resolve(&d.host, SocketAddr::new(d.ip, d.porta))
            .user_agent(USER_AGENT)
            .build()?;
        let r = cliente.get(d.url.clone()).header("Accept", "*/*").send().await?;
        if r.status().is_redirection() {
            let destino = r
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| UrlBloqueada("redirecionamento sem destino".into()))?;
            atual = d.url.join(destino).map_err(|e| UrlBloqueada(format!("URL inválida: {e}")))?.to_string();
            continue;
        }
        let status = r.status().as_u16();
        let tipo = r
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let mut corpo = vec![];
        let mut truncado = false;
        let mut s = r.bytes_stream();
        while let Some(p) = s.next().await {
            corpo.extend_from_slice(&p?);
            if corpo.len() > max_bytes {
                corpo.truncate(max_bytes);
                truncado = true;
                break;
            }
        }
        return Ok(Resposta { status, url_final: d.url.to_string(), tipo, corpo, truncado });
    }
    Err(UrlBloqueada(format!("redirecionamentos demais (>{MAX_REDIRECTS})")).into())
}

pub fn e_texto(tipo: &str) -> bool {
    tipo.is_empty() || TIPOS_TEXTO.iter().any(|t| tipo.starts_with(t) || tipo.ends_with(t))
}

const IGNORAR: [&str; 6] = ["script", "style", "noscript", "template", "svg", "head"];
const QUEBRA: [&str; 13] = ["p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article"];

/// (título, texto legível) de um HTML: sem script, estilo nem marcação.
pub fn html_para_texto(html: &str) -> (String, String) {
    let mut partes = String::new();
    let mut titulo = String::new();
    let mut fora = 0usize;
    let mut no_titulo = false;
    let mut resto = html;
    let dados = |t: &str, fora: usize, no_titulo: bool, partes: &mut String, titulo: &mut String| {
        let t = html_escape::decode_html_entities(t);
        if no_titulo {
            titulo.push_str(&t);
        } else if fora == 0 {
            partes.push_str(&t);
        }
    };
    while let Some(i) = resto.find('<') {
        dados(&resto[..i], fora, no_titulo, &mut partes, &mut titulo);
        resto = &resto[i..];
        if resto.starts_with("<!--") {
            resto = resto.find("-->").map(|j| &resto[j + 3..]).unwrap_or("");
            continue;
        }
        let Some(fim) = resto.find('>') else { break };
        let tag = &resto[1..fim];
        resto = &resto[fim + 1..];
        let fecha = tag.starts_with('/');
        let nome: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if nome.is_empty() {
            continue;
        }
        if nome == "title" {
            no_titulo = !fecha;
        }
        if IGNORAR.contains(&nome.as_str()) {
            if fecha {
                fora = fora.saturating_sub(1);
            } else if !tag.ends_with('/') {
                fora += 1;
                // conteúdo de script e estilo não é HTML: pula até o fechamento
                if nome == "script" || nome == "style" {
                    let alvo = format!("</{nome}");
                    if let Some(j) = resto.to_ascii_lowercase().find(&alvo) {
                        resto = &resto[j..];
                    }
                }
            }
        } else if QUEBRA.contains(&nome.as_str()) {
            partes.push('\n');
        }
    }
    dados(resto, fora, no_titulo, &mut partes, &mut titulo);
    let mut saida: Vec<String> = vec![];
    for ln in partes.lines() {
        let ln = ln.split_whitespace().collect::<Vec<_>>().join(" ");
        if !ln.is_empty() || saida.last().is_some_and(|l| !l.is_empty()) {
            saida.push(ln);
        }
    }
    let texto = saida.join("\n").trim().to_string();
    (titulo.split_whitespace().collect::<Vec<_>>().join(" "), texto)
}
