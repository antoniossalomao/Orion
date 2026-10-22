//! Componentes base (§6): botões, chips, pílulas, selos, segmentado, switch, cartão, linhas de configuração.

use crate::app::Msg;
use crate::icons::{icon, icon_box};
use crate::theme as t;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Background, Border, Color, Element, Length, Padding, Shadow, Theme};

pub type El<'a> = Element<'a, Msg>;

pub fn tx<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> text::Text<'a, Theme> {
    text(s).size(size).color(color).font(t::UI).line_height(text::LineHeight::Relative(1.5))
}
pub fn txf<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color, font: iced::Font) -> text::Text<'a, Theme> {
    text(s).size(size).color(color).font(font).line_height(text::LineHeight::Relative(1.5))
}
/// Corta com reticências conforme a largura disponível (estimativa de 0,53 em por caractere).
/// Texto sem cor própria: herda a cor do botão (muda no hover).
pub fn txi<'a>(s: impl text::IntoFragment<'a>, size: f32, font: iced::Font) -> text::Text<'a, Theme> {
    text(s).size(size).font(font).line_height(text::LineHeight::Relative(1.5))
}
pub fn ell(s: &str, max_px: f32, size: f32) -> String {
    let max = (max_px / (size * 0.53)).floor() as usize;
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(max.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}
/// Barra de rolagem fina (`scrollbar-width: thin`).
pub fn thin_scroll(_: &Theme, s: iced::widget::scrollable::Status) -> iced::widget::scrollable::Style {
    use iced::widget::scrollable::{AutoScroll, Rail, Scroller, Status, Style};
    let shown = matches!(s, Status::Hovered { .. } | Status::Dragged { .. });
    let rail = Rail { background: None, border: Border::default(), scroller: Scroller { background: Background::Color(if shown { t::LINE2 } else { Color::TRANSPARENT }), border: Border { radius: 2.0.into(), ..Default::default() } } };
    Style { container: container::Style::default(), vertical_rail: rail, horizontal_rail: rail, gap: None, auto_scroll: AutoScroll { background: Background::Color(t::RAISE), border: Border::default(), shadow: Shadow::default(), icon: t::FG2 } }
}
pub fn thin_bar() -> iced::widget::scrollable::Scrollbar {
    iced::widget::scrollable::Scrollbar::new().width(4).scroller_width(4).margin(1)
}
pub fn sp_w<'a>(w: f32) -> El<'a> {
    Space::new().width(Length::Fixed(w)).into()
}
pub fn sp_h<'a>(h: f32) -> El<'a> {
    Space::new().height(Length::Fixed(h)).into()
}
pub fn grow<'a>() -> El<'a> {
    Space::new().width(Length::Fill).into()
}
pub fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
}

pub fn surface(bg: Option<Color>, line: Option<Color>, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: bg.map(Background::Color),
        border: Border { color: line.unwrap_or(Color::TRANSPARENT), width: if line.is_some() { 1.0 } else { 0.0 }, radius: radius.into() },
        ..Default::default()
    }
}

/// Sombra dos popovers e modais. O renderizador `tiny-skia` (padrão, abre em < 50 ms) desenha sombra com desfoque como
/// retângulos pretos quando só parte da janela é redesenhada, então aqui não há sombra: a borda e o véu fazem o papel.
pub fn shadow() -> Shadow {
    Shadow::default()
}

pub fn hline<'a>() -> El<'a> {
    container(Space::new()).width(Length::Fill).height(Length::Fixed(1.0)).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}
pub fn vline<'a>(h: f32) -> El<'a> {
    container(Space::new()).width(Length::Fixed(1.0)).height(Length::Fixed(h)).style(|_| container::Style { background: Some(t::LINE2.into()), ..Default::default() }).into()
}

fn btn_style(bg: Option<Color>, hover: Option<Color>, fg: Color, hover_fg: Color, line: Color, hover_line: Color, radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, st| {
        let hov = matches!(st, button::Status::Hovered | button::Status::Pressed);
        let dis = matches!(st, button::Status::Disabled);
        let b = if hov { hover.or(bg) } else { bg };
        button::Style {
            background: b.map(Background::Color),
            text_color: if hov { hover_fg } else { fg },
            border: Border { color: if hov { hover_line } else { line }, width: if line.a > 0.0 || hover_line.a > 0.0 { 1.0 } else { 0.0 }, radius: radius.into() },
            shadow: Shadow::default(),
            snap: true,
        }
        .with_alpha(if dis { 0.3 } else { 1.0 })
    }
}

trait WithAlpha {
    fn with_alpha(self, a: f32) -> Self;
}
impl WithAlpha for button::Style {
    fn with_alpha(mut self, a: f32) -> Self {
        if a < 1.0 {
            self.text_color = t::alpha(self.text_color, a);
            if let Some(Background::Color(c)) = self.background {
                self.background = Some(Background::Color(t::alpha(c, a)));
            }
        }
        self
    }
}

/// Botão de ícone `.ib` (30×30, raio 8). A cor do ícone acompanha o hover.
pub fn ib<'a>(name: &str, msg: Option<Msg>, pressed: bool) -> El<'a> {
    ib_sz(name, 30.0, 16.0, msg, pressed)
}
pub fn ib_sz<'a>(name: &str, size: f32, glyph: f32, msg: Option<Msg>, pressed: bool) -> El<'a> {
    let (idle, hover) = if pressed { (t::FG, t::FG) } else if msg.is_none() { (t::alpha(t::FG3, 0.3), t::alpha(t::FG3, 0.3)) } else { (t::FG3, t::FG) };
    let mut b = button(icon_box(name, size, glyph, idle, hover)).width(Length::Fixed(size)).height(Length::Fixed(size)).padding(0);
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    let bg = if pressed { Some(t::SEL) } else { None };
    b.style(btn_style(bg, Some(if pressed { t::SEL } else { t::HOVER }), t::FG3, t::FG, Color::TRANSPARENT, Color::TRANSPARENT, 8.0)).into()
}
/// Botão de ícone com largura e altura próprias (controles da janela).
pub fn ib_wh<'a>(name: &str, w: f32, h: f32, glyph: f32, msg: Option<Msg>, hover_bg: Color, hover_fg: Color) -> El<'a> {
    let _ = h;
    let inner = container(icon_box(name, w, glyph, t::FG3, hover_fg)).height(Length::Fixed(30.0)).center_y(Length::Fixed(30.0));
    let mut b = button(inner).width(Length::Fixed(w)).height(Length::Fixed(30.0)).padding(0);
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    b.style(btn_style(None, Some(hover_bg), t::FG3, hover_fg, Color::TRANSPARENT, Color::TRANSPARENT, 8.0)).into()
}

/// Dica ao passar o mouse (`title` do protótipo).
pub fn tip<'a>(el: impl Into<El<'a>>, label: &str) -> El<'a> {
    iced::widget::tooltip(el, container(tx(label.to_string(), 11.5, t::FG)).padding(pad(4.0, 8.0, 4.0, 8.0)).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: 8.0.into() }, ..Default::default() }), iced::widget::tooltip::Position::Bottom)
        .gap(6)
        .delay(std::time::Duration::from_millis(500))
        .into()
}

/// Entrada com opacidade (`fadein`): véu da cor do fundo que some com `p` de 0 a 1.
pub fn fade<'a>(el: impl Into<El<'a>>, p: f32, bg: Color) -> El<'a> {
    if p >= 0.999 {
        return el.into();
    }
    let veil = container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| container::Style { background: Some(t::alpha(bg, 1.0 - p).into()), ..Default::default() });
    iced::widget::stack![el.into(), veil].into()
}
/// Deslocamento de entrada (`translate: 0 Npx`): começa `dy` px abaixo e sobe até a posição.
pub fn rise<'a>(el: impl Into<El<'a>>, p: f32, dy: f32) -> El<'a> {
    if p >= 0.999 {
        return el.into();
    }
    container(el).padding(pad(dy * (1.0 - p), 0.0, 0.0, 0.0)).into()
}

/// Borda tracejada (`border: 1px dashed`), desenhada por cima do conteúdo.
pub fn dashed<'a>(el: impl Into<El<'a>>, radius: f32, color: Color) -> El<'a> {
    let c = iced::widget::canvas(Dashed { radius, color }).width(Length::Fill).height(Length::Fill);
    iced::widget::stack![el.into(), c].into()
}
struct Dashed {
    radius: f32,
    color: Color,
}
impl iced::widget::canvas::Program<Msg> for Dashed {
    type State = ();
    fn draw(&self, _s: &(), r: &iced::Renderer, _t: &Theme, b: iced::Rectangle, _c: iced::mouse::Cursor) -> Vec<iced::widget::canvas::Geometry> {
        use iced::widget::canvas::{path::Path, LineDash, Stroke};
        let mut f = iced::widget::canvas::Frame::new(r, b.size());
        let p = Path::rounded_rectangle(iced::Point::new(0.5, 0.5), iced::Size::new(b.width - 1.0, b.height - 1.0), self.radius.into());
        f.stroke(&p, Stroke { width: 1.0, style: self.color.into(), line_dash: LineDash { segments: &[4.0, 3.0], offset: 0 }, ..Stroke::default() });
        vec![f.into_geometry()]
    }
}

/// `letter-spacing`: intercala espaços finos entre as letras.
pub fn spaced(s: &str, hair: usize) -> String {
    let sep: String = std::iter::repeat('\u{200A}').take(hair).collect();
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(&sep)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Normal,
    Primary,
    Danger,
    Ghost,
}

/// Botão `.btn` (32 px) ou `.btn-sm` (28 px).
pub fn btn<'a>(label: &str, kind: Kind, small: bool, ic: Option<&str>, msg: Option<Msg>) -> El<'a> {
    let (fg, bg, hbg, line, hline_) = match kind {
        Kind::Normal => (t::FG, None, Some(t::HOVER), t::LINE2, t::LINE3),
        Kind::Primary => (t::PRIMARY_INK, Some(t::PRIMARY), Some(Color { r: (t::PRIMARY.r * 1.06).min(1.0), g: (t::PRIMARY.g * 1.06).min(1.0), b: (t::PRIMARY.b * 1.06).min(1.0), a: 1.0 }), Color::TRANSPARENT, Color::TRANSPARENT),
        Kind::Danger => (t::DANGER, None, Some(t::DANGER_SOFT), t::mix(t::DANGER, 0.4), t::DANGER),
        Kind::Ghost => (t::FG2, None, None, Color::TRANSPARENT, Color::TRANSPARENT),
    };
    let hfg = if kind == Kind::Ghost { t::FG } else { fg };
    let size = if small { 12.5 } else { 13.0 };
    let mut r = row![].spacing(6).align_y(Vertical::Center);
    if let Some(i) = ic {
        r = r.push(icon(i, if small { 14.0 } else { 16.0 }, fg));
    }
    r = r.push(if matches!(kind, Kind::Normal | Kind::Ghost) { txi(label.to_string(), size, t::UI_MED) } else { txf(label.to_string(), size, fg, t::UI_MED) });
    let h = if small { 28.0 } else { 32.0 };
    let mut b = button(container(r).center_y(Length::Fill)).height(Length::Fixed(h)).padding(pad(0.0, if small { 10.0 } else { 12.0 }, 0.0, if small { 10.0 } else { 12.0 }));
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    b.style(btn_style(bg, hbg, fg, hfg, line, hline_, 8.0)).into()
}

/// Chip `.chip` (28 px, raio 7).
pub fn chip<'a>(label: String, soft: Option<String>, lead: Option<&str>, chev: bool, msg: Msg) -> El<'a> {
    let mut r = row![].spacing(6).align_y(Vertical::Center);
    if let Some(i) = lead {
        r = r.push(icon(i, 14.0, t::FG2));
    }
    r = r.push(txi(label, 12.5, t::UI));
    if let Some(s) = soft {
        r = r.push(tx(s, 12.5, t::FG3));
    }
    if chev {
        r = r.push(icon("chevD", 14.0, t::FG3));
    }
    button(container(r).center_y(Length::Fill)).height(Length::Fixed(28.0)).padding(pad(0.0, 8.0, 0.0, 8.0)).on_press(msg).style(btn_style(None, Some(t::HOVER), t::FG2, t::FG, Color::TRANSPARENT, Color::TRANSPARENT, 7.0)).into()
}

/// Pílula `.pill` da barra superior.
pub fn pill<'a>(dot: Color, label: &str, border: Color, strong: bool, msg: Option<Msg>) -> El<'a> {
    let inner = row![dot_el(dot, 6.0), tx(label.to_string(), 12.0, if strong { t::FG } else { t::FG2 })].spacing(7).align_y(Vertical::Center);
    let c = container(inner).height(Length::Fixed(26.0)).center_y(Length::Fixed(26.0)).padding(pad(0.0, 10.0, 0.0, 10.0)).style(surface(None, Some(border), 999.0));
    match msg {
        Some(m) => mouse_area(c).on_press(m).into(),
        None => c.into(),
    }
}

pub fn dot_el<'a>(c: Color, d: f32) -> El<'a> {
    container(Space::new()).width(Length::Fixed(d)).height(Length::Fixed(d)).style(move |_| container::Style { background: Some(c.into()), border: Border { radius: (d / 2.0).into(), ..Default::default() }, ..Default::default() }).into()
}
/// Ponto de estado `.cdot.pending` com halo.
pub fn dot_halo<'a>(c: Color, halo: Color) -> El<'a> {
    container(dot_el(c, 6.0)).padding(3).style(move |_| container::Style { background: Some(halo.into()), border: Border { radius: 99.0.into(), ..Default::default() }, ..Default::default() }).into()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    Ok,
    Warn,
    Err,
    Info,
}
/// Selo `.badge`.
pub fn badge<'a>(label: &str, tone: Tone) -> El<'a> {
    let (fg, bg, line) = match tone {
        Tone::Plain => (t::FG2, None, Some(t::LINE2)),
        Tone::Ok => (t::OK, Some(t::OK_SOFT), None),
        Tone::Warn => (t::SIGNAL, Some(t::SIGNAL_SOFT), None),
        Tone::Err => (t::DANGER, Some(t::DANGER_SOFT), None),
        Tone::Info => (t::ACCENT, Some(t::ACCENT_SOFT), None),
    };
    container(txf(label.to_string(), 11.0, fg, t::UI_MED)).padding(pad(1.0, 8.0, 1.0, 8.0)).style(surface(bg, line, 999.0)).into()
}
pub fn badge_ic<'a>(label: &str, tone: Tone, ic: &str) -> El<'a> {
    let fg = match tone {
        Tone::Ok => t::OK,
        Tone::Warn => t::SIGNAL,
        Tone::Err => t::DANGER,
        Tone::Info => t::ACCENT,
        Tone::Plain => t::FG2,
    };
    let bg = match tone {
        Tone::Ok => Some(t::OK_SOFT),
        Tone::Warn => Some(t::SIGNAL_SOFT),
        Tone::Err => Some(t::DANGER_SOFT),
        Tone::Info => Some(t::ACCENT_SOFT),
        Tone::Plain => None,
    };
    let line = if tone == Tone::Plain { Some(t::LINE2) } else { None };
    container(row![icon(ic, 12.0, fg), txf(label.to_string(), 11.0, fg, t::UI_MED)].spacing(5).align_y(Vertical::Center)).padding(pad(1.0, 8.0, 1.0, 8.0)).style(surface(bg, line, 999.0)).into()
}

pub fn kbd<'a>(s: &str) -> El<'a> {
    container(txf(s.to_string(), 10.5, t::FG3, t::MONO)).padding(pad(1.0, 5.0, 1.0, 5.0)).style(surface(None, Some(t::LINE2), 5.0)).into()
}

/// Segmentado `.seg`.
pub fn seg<'a>(opts: &[&str], sel: usize, f: impl Fn(usize) -> Msg) -> El<'a> {
    let mut r = row![].spacing(2);
    for (i, o) in opts.iter().enumerate() {
        let on = i == sel;
        let b = button(container(txi(o.to_string(), 12.5, t::UI)).center_y(Length::Fill))
            .height(Length::Fixed(28.0))
            .padding(pad(0.0, 12.0, 0.0, 12.0))
            .on_press(f(i))
            .style(btn_style(if on { Some(t::SEL) } else { None }, if on { Some(t::SEL) } else { None }, if on { t::FG } else { t::FG3 }, t::FG, Color::TRANSPARENT, Color::TRANSPARENT, 8.0));
        r = r.push(b);
    }
    container(r).padding(2).style(surface(None, Some(t::LINE2), 10.0)).into()
}

/// Switch `.sw` (34×20).
pub fn switch<'a>(on: bool, msg: Msg) -> El<'a> {
    // a bolinha desliza 14 px em 200 ms; só o último switch tocado anima
    let moving = crate::anim::SWITCH_KEY.lock().map(|k| *k == format!("{msg:?}")).unwrap_or(false);
    let p = if moving { crate::anim::SWITCH.get() } else { 1.0 };
    let off = 14.0 * if on { p } else { 1.0 - p };
    let thumb = container(Space::new()).width(Length::Fixed(16.0)).height(Length::Fixed(16.0)).style(|_| container::Style {
        background: Some(Color::WHITE.into()),
        border: Border { radius: 8.0.into(), ..Default::default() },
        ..Default::default()
    });
    let track = container(thumb)
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(20.0))
        .padding(pad(2.0, 2.0, 2.0, 2.0 + off))
        .style(move |_| container::Style { background: Some(if on { t::ACCENT } else { t::LINE3 }.into()), border: Border { radius: 999.0.into(), ..Default::default() }, ..Default::default() });
    mouse_area(track).on_press(msg).into()
}

/// Seletor em caixa `.sel.box`.
pub fn sel_box<'a>(label: String, min_w: f32, msg: Msg) -> El<'a> {
    let r = row![tx(label, 13.0, t::FG), grow(), icon("chevD", 14.0, t::FG3)].spacing(6).align_y(Vertical::Center);
    button(container(r).center_y(Length::Fill)).width(Length::Fixed(min_w)).height(Length::Fixed(32.0)).padding(pad(0.0, 8.0, 0.0, 12.0)).on_press(msg).style(btn_style(None, Some(t::HOVER), t::FG, t::FG, t::LINE2, t::LINE2, 8.0)).into()
}

/// Cartão `.card` do painel da direita.
pub fn card<'a>(title: &str, right: El<'a>, inner: El<'a>) -> El<'a> {
    let head = row![txf(title.to_string(), 12.5, t::FG, t::UI_MED), grow(), right].align_y(Vertical::Center).padding(pad(10.0, 12.0, 10.0, 12.0));
    container(column![head, container(inner).padding(pad(0.0, 12.0, 12.0, 12.0))]).width(Length::Fill).style(surface(Some(t::RAISE), Some(t::LINE), 12.0)).into()
}

/// Medidor `.meter` (altura 3) ou `.hbar` (altura 5).
pub fn meter<'a>(pct: f32, h: f32, color: Color) -> El<'a> {
    let pct = pct.clamp(0.0, 100.0);
    let fill = container(Space::new()).width(Length::FillPortion((pct * 10.0) as u16 + 1)).height(Length::Fill).style(move |_| container::Style { background: Some(color.into()), border: Border { radius: 3.0.into(), ..Default::default() }, ..Default::default() });
    let rest = Space::new().width(Length::FillPortion(((100.0 - pct) * 10.0) as u16 + 1));
    let inner: El = if pct >= 100.0 { container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| container::Style { background: Some(color.into()), border: Border { radius: 3.0.into(), ..Default::default() }, ..Default::default() }).into() } else if pct <= 0.0 { Space::new().into() } else { row![fill, rest].into() };
    container(inner).width(Length::Fill).height(Length::Fixed(h)).style(surface(Some(t::RAISE2), None, 3.0)).into()
}

/// Linha de configuração `.row`: título, descrição e controles à direita.
pub fn setting<'a>(title: &str, desc: &str, ctl: El<'a>) -> El<'a> {
    setting_el(tx(title.to_string(), t::TEXT_BASE, t::FG).into(), desc, ctl)
}
pub fn setting_el<'a>(title: El<'a>, desc: &str, ctl: El<'a>) -> El<'a> {
    let mut left = column![title].spacing(2).width(Length::Fill);
    if !desc.is_empty() {
        left = left.push(container(tx(desc.to_string(), 12.5, t::FG3)).max_width(430.0));
    }
    let r = row![left, ctl].spacing(24).align_y(Vertical::Center).padding(pad(15.0, 0.0, 15.0, 0.0));
    column![r, hline()].into()
}

pub fn click<'a>(e: impl Into<El<'a>>, m: Msg) -> El<'a> {
    mouse_area(e).on_press(m).into()
}

pub fn center_x<'a>(e: impl Into<El<'a>>) -> El<'a> {
    container(e).width(Length::Fill).center_x(Length::Fill).into()
}

/// Cinturão (marca): 3 pontos, o primeiro sobe e o terceiro desce.
pub fn belt<'a>(large: bool, colors: [Color; 3]) -> El<'a> {
    let (d, gap, off) = if large { (7.0, 6.0, 6.0) } else { (4.0, 3.0, 3.0) };
    let h = d + off * 2.0;
    let dot = |c: Color, dy: f32| -> El<'a> { container(dot_el(c, d)).padding(pad(dy, 0.0, 0.0, 0.0)).into() };
    container(row![dot(colors[0], 0.0), dot(colors[1], off), dot(colors[2], off * 2.0)].spacing(gap)).height(Length::Fixed(h)).into()
}
pub fn belt_fg<'a>(large: bool) -> El<'a> {
    belt(large, [t::FG, t::FG, t::FG])
}

/// Selo da classe de risco da ação aprovada; `None` se o servidor não informou.
fn risk_badge<'a>(risk: &str) -> Option<El<'a>> {
    Some(match risk {
        "destructive" => badge("Ação destrutiva", Tone::Err),
        "exec" => badge("Executa no computador", Tone::Warn),
        "write" => badge("Escreve", Tone::Warn),
        "read" => badge("Leitura", Tone::Plain),
        _ => return None,
    })
}

/// Corpo do cartão de aprovação: classe de risco, ferramenta e TODOS os argumentos
/// (REGRAS.md: só se aprova o que se vê inteiro).
pub fn approval_detail<'a>(a: &crate::data::Approval) -> El<'a> {
    let mut col = column![].spacing(8);
    let mut head = row![].spacing(8).align_y(Vertical::Center);
    let mut tem_cabeca = false;
    if let Some(b) = risk_badge(&a.risk) {
        head = head.push(b);
        tem_cabeca = true;
    }
    if !a.tool.is_empty() {
        head = head.push(txf(a.tool.clone(), 12.0, t::FG2, t::MONO));
        tem_cabeca = true;
    }
    if tem_cabeca {
        col = col.push(head);
    }
    if !a.args.is_empty() {
        let mut linhas = column![].spacing(3);
        for (k, v) in &a.args {
            linhas = linhas.push(row![txf(format!("{k}:"), 12.0, t::FG3, t::MONO), txf(v.clone(), 12.0, t::FG, t::MONO)].spacing(6));
        }
        col = col.push(container(linhas).width(Length::Fill).padding(pad(8.0, 10.0, 8.0, 10.0)).style(surface(Some(t::RAISE), Some(t::LINE2), 8.0)));
    }
    if a.truncated {
        col = col.push(tx("Argumentos grandes demais para mostrar inteiros: só dá para negar.", 12.5, t::DANGER));
    }
    col.into()
}

/// Botões do cartão. Nenhum dos dois ganha destaque de "caminho feliz": só a ação destrutiva
/// pende para o lado de negar. Argumentos cortados só podem ser negados.
pub fn approval_actions<'a>(a: &crate::data::Approval, approve: Msg, deny: Msg) -> Vec<El<'a>> {
    let (k_ok, k_no) = if a.risk == "destructive" { (Kind::Danger, Kind::Primary) } else { (Kind::Normal, Kind::Normal) };
    let mut v = vec![];
    if !a.truncated {
        v.push(btn("Aprovar", k_ok, true, None, Some(approve)));
    }
    v.push(btn("Negar", k_no, true, None, Some(deny)));
    v
}
