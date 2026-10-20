//! Leitura de página em Markdown, sem navegador: tira menu, rodapé, anúncios e scripts (Readability),
//! converte o que sobra em Markdown e lista os links. É o substituto em Rust do `crawl4ai`.
//!
//! O resultado continua sendo conteúdo externo não confiável (dado, nunca instrução). Por isso
//! imagens saem do Markdown (nada de pixel de rastreio nem imagem com destino escolhido pela página)
//! e só links http(s) são listados.

use regex::Regex;
use std::sync::LazyLock;

/// Abaixo disto o Readability provavelmente pegou só um pedaço (menu, cookie): usa a página inteira.
const MIN_ARTIGO: usize = 200;
const MAX_LINKS: usize = 40;
const MAX_ELEMENTOS: usize = 60_000;
const PULAR: [&str; 12] = ["script", "style", "noscript", "template", "svg", "head", "img", "picture", "iframe", "canvas", "video", "audio"];
/// Só na página inteira (sem Readability): partes que são navegação, não conteúdo.
const PULAR_PAGINA: [&str; 4] = ["nav", "footer", "aside", "form"];

static IMAGEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]\n]{1,160})\]\((<[^>\s]+>|[^)\s]+)(?:\s+&quot;[^)]*&quot;|\s+\x22[^\x22]*\x22)?\)").unwrap());
static LINK_VAZIO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*\]\([^)]*\)").unwrap());
static TITULO_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\]\(([^)\s]+) "[^"]*"\)"#).unwrap());
static ESPACOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" {2,}").unwrap());
static TRACOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-{4,}").unwrap());
static LINHAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

#[derive(Debug, Clone, Default)]
pub struct Pagina {
    pub titulo: String,
    pub autor: Option<String>,
    pub resumo: Option<String>,
    pub site: Option<String>,
    pub publicado: Option<String>,
    pub markdown: String,
    /// (texto, url absoluta http/https), sem repetição.
    pub links: Vec<(String, String)>,
    /// `true` se o Readability achou um artigo; `false` se foi a página inteira.
    pub artigo: bool,
}

fn markdown_de(html: &str, pular: &[&str]) -> String {
    let conv = htmd::HtmlToMarkdown::builder().skip_tags(pular.to_vec()).build();
    conv.convert(html).unwrap_or_default()
}

fn limpar(md: &str) -> String {
    // a imagem sai e o link que a envolvia sobra vazio; o título do link só repete o texto
    let sem_img = IMAGEM.replace_all(md, "").into_owned();
    let sem_img = LINK_VAZIO.replace_all(&sem_img, "").into_owned();
    let sem_img = TITULO_LINK.replace_all(&sem_img, "]($1)").into_owned();
    // tabelas saem com colunas alinhadas por espaços e traços: só gasta contexto
    let linhas: Vec<String> = sem_img
        .lines()
        .map(|l| {
            let l = l.trim_end();
            if l.starts_with('|') {
                TRACOS.replace_all(&ESPACOS.replace_all(l, " "), "---").into_owned()
            } else {
                l.to_string()
            }
        })
        .collect();
    LINHAS.replace_all(linhas.join("\n").trim(), "\n\n").into_owned()
}

fn so_http(url: &str, base: Option<&url::Url>) -> Option<String> {
    let u = url.trim_matches(['<', '>']);
    let abs = match base {
        Some(b) => b.join(u).ok()?,
        None => url::Url::parse(u).ok()?,
    };
    matches!(abs.scheme(), "http" | "https").then(|| abs.to_string())
}

/// Links `[texto](url)` do Markdown, resolvidos contra a página e sem repetição.
pub fn extrair_links(md: &str, base: &str) -> Vec<(String, String)> {
    let base = url::Url::parse(base).ok();
    let mut vistos = std::collections::HashSet::new();
    let mut out = vec![];
    for c in LINK.captures_iter(md) {
        let texto = c[1].split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(url) = so_http(&c[2], base.as_ref()) else { continue };
        if texto.is_empty() || !vistos.insert(url.clone()) {
            continue;
        }
        out.push((texto, url));
        if out.len() >= MAX_LINKS {
            break;
        }
    }
    out
}

/// Lê um HTML: artigo principal quando existe; senão a página inteira sem a navegação.
pub fn ler_html(html: &str, url: &str) -> Pagina {
    let cfg = dom_smoothie::Config { max_elements_to_parse: MAX_ELEMENTOS, ..Default::default() };
    if let Ok(mut r) = dom_smoothie::Readability::new(html, Some(url), Some(cfg)) {
        if let Ok(a) = r.parse() {
            let md = limpar(&markdown_de(&a.content, &PULAR));
            if md.chars().count() >= MIN_ARTIGO {
                return Pagina {
                    titulo: a.title.split_whitespace().collect::<Vec<_>>().join(" "),
                    autor: a.byline.filter(|x| !x.trim().is_empty()),
                    resumo: a.excerpt.filter(|x| !x.trim().is_empty()),
                    site: a.site_name.filter(|x| !x.trim().is_empty()),
                    publicado: a.published_time.filter(|x| !x.trim().is_empty()),
                    links: extrair_links(&md, url),
                    markdown: md,
                    artigo: true,
                };
            }
        }
    }
    let todos: Vec<&str> = PULAR.iter().chain(PULAR_PAGINA.iter()).copied().collect();
    let md = limpar(&markdown_de(html, &todos));
    let (titulo, _) = crate::netguard::html_para_texto(html);
    Pagina { titulo, links: extrair_links(&md, url), markdown: md, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTIGO: &str = r#"<!doctype html><html><head><title>Como o Orion lê páginas | Blog</title>
<meta name="author" content="Ana Souza"><script>window.track('x')</script><style>.a{color:red}</style></head>
<body><nav><a href="/">Início</a><a href="/sobre">Sobre</a></nav>
<div class="ad">COMPRE AGORA <a href="https://anuncio.example/x">oferta</a></div>
<article><h1>Como o Orion lê páginas</h1>
<p>Primeiro parágrafo com <a href="/detalhes">um link relativo</a> e <strong>destaque</strong>. O Orion baixa a página, tira o que não é conteúdo e entrega um texto limpo para o modelo trabalhar sem gastar contexto à toa com menu e rodapé.</p>
<h2>Etapas</h2><ul><li>Baixar</li><li>Extrair</li><li>Converter</li></ul>
<p>Mais um parágrafo comprido para o Readability considerar isto um artigo de verdade, com bastante texto corrido e frases completas que justifiquem a extração principal da página.</p>
<img src="https://rastreio.example/pixel.png" alt="pixel"><p><a href="javascript:alert(1)">perigo</a> <a href="mailto:a@b.c">email</a></p>
</article><footer>© 2026 Todos os direitos</footer></body></html>"#;

    #[test]
    fn artigo_vira_markdown_sem_menu_rodape_script_nem_imagem() {
        let p = ler_html(ARTIGO, "https://blog.example/post/1");
        assert!(p.artigo, "{p:?}");
        assert!(p.markdown.contains("## Etapas") || p.markdown.contains("Etapas"), "{}", p.markdown);
        assert!(Regex::new(r"(?m)^[-*]\s+Baixar$").unwrap().is_match(&p.markdown), "{}", p.markdown);
        assert!(p.markdown.contains("**destaque**"));
        for fora in ["window.track", "color:red", "COMPRE AGORA", "Todos os direitos", "pixel.png", "!["] {
            assert!(!p.markdown.contains(fora), "{fora} não devia aparecer: {}", p.markdown);
        }
        assert!(p.titulo.contains("Como o Orion lê páginas"));
    }

    #[test]
    fn tabela_nao_leva_espacos_de_alinhamento() {
        let html = "<html><body><table><tr><th>Nome</th><th>Valor longo da coluna</th></tr><tr><td>a</td><td>1</td></tr></table></body></html>";
        let p = ler_html(html, "https://t.example/");
        assert!(!p.markdown.contains("  "), "{:?}", p.markdown);
        assert!(!p.markdown.contains("----"), "{:?}", p.markdown);
        assert!(p.markdown.contains("Nome") && p.markdown.contains("Valor longo da coluna"));
    }

    #[test]
    fn link_de_imagem_e_titulo_de_link_nao_sobram() {
        let html = r#"<html><body><p>Veja <a href="https://x.example/a" title="Título repetido do link">o artigo</a> e <a href="https://x.example/b"><img src="/i.png" alt="logo"></a> pronto, com texto suficiente para ficar na página inteira sem artigo.</p></body></html>"#;
        let p = ler_html(html, "https://x.example/");
        assert!(!p.markdown.contains("[]("), "{}", p.markdown);
        assert!(!p.markdown.contains("Título repetido"), "{}", p.markdown);
        assert!(p.markdown.contains("[o artigo](https://x.example/a)"), "{}", p.markdown);
    }

    #[test]
    fn links_saem_absolutos_e_so_http() {
        let p = ler_html(ARTIGO, "https://blog.example/post/1");
        let urls: Vec<&str> = p.links.iter().map(|l| l.1.as_str()).collect();
        assert!(urls.contains(&"https://blog.example/detalhes"), "{urls:?}");
        assert!(!urls.iter().any(|u| u.starts_with("javascript:") || u.starts_with("mailto:")), "{urls:?}");
    }

    #[test]
    fn pagina_curta_cai_na_pagina_inteira_sem_navegacao() {
        let html = "<html><head><title>Oi</title></head><body><nav><a href='/x'>menu</a></nav><h1>Título</h1><p>Texto curto.</p><footer>rodapé</footer></body></html>";
        let p = ler_html(html, "https://e.example/");
        assert!(!p.artigo);
        assert!(p.markdown.contains("Título") && p.markdown.contains("Texto curto."), "{}", p.markdown);
        assert!(!p.markdown.contains("menu") && !p.markdown.contains("rodapé"), "{}", p.markdown);
        assert_eq!(p.titulo, "Oi");
    }

    #[test]
    fn links_do_markdown_nao_repetem_e_resolvem_relativos() {
        let l = extrair_links("[a](/x) [b](https://o.example/y) [c](/x) [d](data:text/html,1) ![i](/img.png)", "https://s.example/p/");
        let urls: Vec<&str> = l.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(urls, ["https://s.example/x", "https://o.example/y", "https://s.example/img.png"]);
    }
}
