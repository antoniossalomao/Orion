//! Interface em Fluent. Só rótulos do produto são traduzidos; conteúdo do usuário fica intacto
//! (REGRAS.md: conteúdo externo é dado, não instrução; portabilidade).
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

static ENGLISH: AtomicBool = AtomicBool::new(false);
static SOURCES: LazyLock<HashMap<String, String>> = LazyLock::new(|| serde_json::from_str(include_str!("../locales/sources.json")).expect("catálogo de origem válido"));
thread_local! {
    static BUNDLES: [FluentBundle<FluentResource>; 2] = [bundle("pt-BR", include_str!("../locales/pt-BR.ftl")), bundle("en", include_str!("../locales/en.ftl"))];
    static LABELS: std::cell::RefCell<HashMap<(bool, String), &'static str>> = std::cell::RefCell::new(HashMap::new());
}
fn bundle(locale: &str, source: &str) -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(source.to_owned()).expect("Fluent válido");
    let mut bundle = FluentBundle::new(vec![locale.parse().expect("idioma válido")]);
    bundle.set_use_isolating(false);
    bundle.add_resource(resource).expect("IDs Fluent únicos");
    bundle
}
pub fn set_english(english: bool) { ENGLISH.store(english, Ordering::Relaxed); }
pub fn english() -> bool { ENGLISH.load(Ordering::Relaxed) }
fn resolve(source: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
    let id = SOURCES.get(source)?;
    BUNDLES.with(|bundles| {
        let bundle = &bundles[usize::from(english())];
        let message = bundle.get_message(id)?.value()?;
        let mut errors = vec![];
        let text = bundle.format_pattern(message, args, &mut errors).into_owned();
        errors.is_empty().then_some(text)
    })
}
pub fn text(source: &'static str) -> &'static str {
    LABELS.with(|labels| {
        let key = (english(), source.to_owned());
        if let Some(text) = labels.borrow().get(&key) { return *text; }
        let translated = resolve(source, None).unwrap_or_else(|| source.to_owned());
        let text = Box::leak(translated.into_boxed_str()) as &'static str;
        // Chamado apenas com rótulos estáticos, nunca com conteúdo de mensagens ou arquivos.
        labels.borrow_mut().insert(key, text);
        text
    })
}
fn make_pattern(source: &str) -> regex::Regex {
    let mut regex = String::from("(?s)^" );
    let mut rest = source;
    while let Some(start) = rest.find('{') {
        regex.push_str(&regex::escape(&rest[..start]));
        let Some(end) = rest[start..].find('}') else { break };
        regex.push_str("(.*?)");
        rest = &rest[start + end + 1..];
    }
    regex.push_str(&regex::escape(rest)); regex.push('$');
    regex::Regex::new(&regex).expect("padrão gerado válido")
}
static PATTERNS: LazyLock<HashMap<String, regex::Regex>> = LazyLock::new(|| serde_json::from_str::<Vec<String>>(include_str!("../locales/templates.json")).expect("templates válidos").into_iter().map(|source| { let pattern = make_pattern(&source); (source, pattern) }).collect());
pub fn formatted(source: &str, rendered: String) -> String {
    if !english() { return rendered; }
    let Some(regex) = PATTERNS.get(source) else { return resolve(source, None).unwrap_or(rendered) };
    let Some(captures) = regex.captures(&rendered) else { return rendered };
    let mut args = FluentArgs::new();
    for (index, capture) in captures.iter().skip(1).enumerate() { args.set(format!("v{index}"), capture.map_or("", |m| m.as_str())); }
    resolve(source, Some(&args)).unwrap_or(rendered)
}
/// Erros e estados do próprio produto; não usado no corpo de conversas, títulos ou documentos.
pub fn status(source: &str) -> String {
    if !english() { return source.into(); }
    if !source.contains(['{', '}']) { if let Some(text) = resolve(source, None) { return text; } }
    static ORDERED: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        let mut templates = PATTERNS.keys().map(String::as_str).collect::<Vec<_>>();
        templates.sort_by_key(|source| std::cmp::Reverse(source.len()));
        templates
    });
    for template in ORDERED.iter().copied() {
        let pattern = &PATTERNS[template];
        if pattern.is_match(source) { return formatted(template, source.into()); }
    }
    source.into()
}
