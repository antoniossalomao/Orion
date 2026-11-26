//! Regressão A5–A8 do QA: um clique não pode sumir quando uma mensagem (Tick, Poll, WindowActive)
//! chega entre apertar e soltar o botão e a animação de entrada termina nesse intervalo.
//! A árvore de widgets precisa ter a mesma forma antes e depois (fade/rise com estrutura fixa).
#![cfg(feature = "snap")]
use iced::{mouse, Event, Point, Settings, Size};
use iced_test::runtime::user_interface::Cache;
use iced_test::runtime::UserInterface;
use iced_test::simulator::Simulator;
use iced_test::core::clipboard;
use iced_test::core::renderer::Headless;
use orion_app::app::{App, Msg, Sec, View};
use std::time::{Duration, Instant};

const SIZE: Size = Size::new(1440.0, 900.0);

fn centro(app: &App, texto: &str) -> Point {
    let settings = Settings { fonts: orion_app::theme::fonts(), default_font: orion_app::theme::UI, ..Settings::default() };
    let mut sim = Simulator::with_size(settings, SIZE, app.view());
    let alvo = sim.find(texto).unwrap_or_else(|e| panic!("{texto}: {e:?}"));
    use iced_test::selector::Bounded;
    alvo.visible_bounds().expect("visível").center()
}

/// Aperta com a tela de `app`, deixa o relógio andar `depois`, solta numa interface reconstruída e devolve as mensagens.
fn clicar_com_mensagem_no_meio(app: &mut App, texto: &str, depois: Duration) -> Vec<String> {
    std::env::set_var("ICED_TEST_BACKEND", "tiny-skia");
    let ponto = centro(app, texto);
    let cursor = mouse::Cursor::Available(ponto);
    let mut renderer = iced_test::futures::futures::executor::block_on(<iced_test::renderer::Renderer as Headless>::new(orion_app::theme::UI, iced::Pixels(16.0), Some("tiny-skia"))).expect("renderer");
    let mut msgs: Vec<Msg> = Vec::new();
    let mut ui = UserInterface::build(app.view(), SIZE, Cache::default(), &mut renderer);
    ui.update(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))], cursor, &mut renderer, &mut clipboard::Null, &mut msgs);
    let cache = ui.into_cache();
    app.now += depois;
    let mut ui = UserInterface::build(app.view(), SIZE, cache, &mut renderer);
    ui.update(&[Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))], cursor, &mut renderer, &mut clipboard::Null, &mut msgs);
    msgs.iter().map(|m| format!("{m:?}")).collect()
}

fn logado() -> App {
    let mut a = orion_app::snap::logged();
    a.now = Instant::now();
    a
}

#[test]
fn novo_projeto_nao_perde_o_clique() {
    let mut a = logado();
    a.update(Msg::Go(View::Projetos));
    let m = clicar_com_mensagem_no_meio(&mut a, "Novo projeto", Duration::from_secs(2));
    assert!(m.contains(&"ProjectStart".to_string()), "{m:?}");
}

#[test]
fn configuracoes_nao_perde_o_clique_na_animacao_de_entrada() {
    let mut a = logado();
    a.update(Msg::OpenSettings(Some(Sec::Geral)));
    let m = clicar_com_mensagem_no_meio(&mut a, "Compacta", Duration::from_secs(2));
    assert!(m.iter().any(|x| x.contains("density")), "{m:?}");
}
