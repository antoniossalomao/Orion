//! Ícones do protótipo: SVG 24×24, traço de 1,55, pontas arredondadas, sem preenchimento (§6, §7).

use iced::widget::svg::{self, Handle, Svg};
use iced::{Color, Length};
use std::collections::HashMap;
use std::sync::OnceLock;

const PATHS: &[(&str, &str)] = &[
    ("menu", "M4 7h16M4 12h16M4 17h16"),
    ("panelL", "M4 5h16v14H4zM9.5 5v14"),
    ("panelR", "M4 5h16v14H4zM14.5 5v14"),
    ("back", "M15 6l-6 6 6 6"),
    ("fwd", "M9 6l6 6-6 6"),
    ("search", "M11 18a7 7 0 1 1 0-14 7 7 0 0 1 0 14zM20 20l-4-4"),
    ("plus", "M12 5v14M5 12h14"),
    ("minus", "M5 12h14"),
    ("folder", "M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2h8.5A1.5 1.5 0 0 1 21 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"),
    ("result", "M8 4h8a2 2 0 0 1 2 2v14l-6-3-6 3V6a2 2 0 0 1 2-2z"),
    ("chevD", "M6 9l6 6 6-6"),
    ("chevR", "M9 6l6 6-6 6"),
    ("graph", "M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM5 7a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM19 8a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM6 20a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM18 19a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM6 6.5l4.5 4M17.8 7.3l-4.3 3.4M7 17.5l3.5-4M16.8 16.6l-3.4-3.2"),
    ("gear", "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"),
    ("mic", "M12 15a3 3 0 0 0 3-3V6a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3zM6 11a6 6 0 0 0 12 0M12 17v4"),
    ("wave", "M4 12v0M8 8v8M12 5v14M16 8v8M20 12v0"),
    ("copy", "M9 9h11v11H9zM5 15H4V4h11v1"),
    ("speaker", "M11 5 6 9H3v6h3l5 4zM15.5 8.5a5 5 0 0 1 0 7"),
    ("retry", "M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5"),
    ("close", "M6 6l12 12M18 6L6 18"),
    ("check", "M5 12.5l4.5 4.5L19 7.5"),
    ("up", "M12 19V5M6 11l6-6 6 6"),
    ("pin", "M9 4h6l-1 6 3 3v1.5H7V13l3-3zM12 14.5V20"),
    ("user", "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0"),
    ("shield", "M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z"),
    ("gauge", "M12 14l4-4M4 18a9 9 0 1 1 16 0"),
    ("plug", "M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4"),
    ("db", "M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"),
    ("cpu", "M7 7h10v10H7zM10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"),
    ("keys", "M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10"),
    ("logout", "M15 4h4v16h-4M10 8l-4 4 4 4M6 12h10"),
    ("trash", "M4 7h16M9 7V4h6v3M6.5 7l1 13h9l1-13"),
    ("edit", "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"),
    ("bell", "M6 17v-6a6 6 0 0 1 12 0v6l2 2H4zM10 21h4"),
    ("branch", "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9"),
    ("file", "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5"),
    ("book", "M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2zM4 21V5M8 7h7"),
    ("archive", "M3 4h18v4H3zM5 8v12h14V8M10 12h4"),
    ("min", "M6 12h12"),
    ("max", "M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5"),
    ("fit", "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"),
    ("refresh", "M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5"),
    ("chat", "M4 5h16v11H9l-5 4z"),
    ("clock", "M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18zM12 7v5l3 2"),
    ("inbox", "M4 13l2.5-7h11L20 13v6H4zM4 13h5l1 2h4l1-2h5"),
    ("cloud", "M7 18a4 4 0 0 1-.6-8A6 6 0 0 1 18 9a4.5 4.5 0 0 1-.5 9z"),
    ("send2", "M21 4L3 11l6 2.5L11.5 20l3-5 6.5-11z"),
    ("camera", "M4 8h3l2-3h6l2 3h3v11H4zM12 17a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z"),
    ("out", "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"),
    ("term", "M4 5h16v14H4zM8 10l3 2-3 2M13 15h3"),
    ("diff", "M6 3v12M6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9v12M15 18h6"),
    ("play", "M8 5l11 7-11 7z"),
    ("image", "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M15 9h.01"),
    ("moon", "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"),
    ("eye", "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"),
    ("globe", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"),
    ("bolt", "M13 3L5 14h6l-1 7 8-11h-6z"),
    ("zap", "M4 12h16M12 4v16"),
    ("undo", "M9 14L4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3"),
    ("scan", "M4 8V4h4M20 8V4h-4M4 16v4h4M20 16v4h-4M9 12h6"),
    ("heart", "M12 20s-7-4.5-7-10a4 4 0 0 1 7-2.5A4 4 0 0 1 19 10c0 5.5-7 10-7 10z"),
    ("puzzle", "M10 4h4v3a2 2 0 1 0 4 0V4h2v6h-3a2 2 0 1 0 0 4h3v6h-6v-3a2 2 0 1 0-4 0v3H4v-6h3a2 2 0 1 0 0-4H4V4z"),
];

fn handles() -> &'static HashMap<&'static str, Handle> {
    static H: OnceLock<HashMap<&'static str, Handle>> = OnceLock::new();
    H.get_or_init(|| {
        PATHS
            .iter()
            .map(|(n, d)| {
                let s = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round"><path d="{d}"/></svg>"##
                );
                (*n, Handle::from_memory(s.into_bytes()))
            })
            .collect()
    })
}

/// Ícone `nome` com `size` px e a cor dada (o SVG é pintado com o filtro de cor do iced).
pub fn icon<'a>(name: &str, size: f32, color: Color) -> Svg<'a> {
    let h = handles().get(name).cloned().unwrap_or_else(|| handles()["close"].clone());
    svg::Svg::new(h)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}

/// Quadrado preenchido do botão "Parar".
pub fn stop_square<'a>(size: f32, color: Color) -> Svg<'a> {
    let s = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="7" y="7" width="10" height="10" rx="2" fill="#000"/></svg>"##;
    svg::Svg::new(Handle::from_memory(s.as_bytes().to_vec()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
