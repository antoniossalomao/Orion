#![allow(unused_must_use)]

//! Cenários de captura (PNG por tela e estado), com os dados de exemplo da §28.1, renderizados sem janela.
//! Os mesmos cenários existem em `tools/cenas.js` para tirar a foto do protótipo HTML e comparar lado a lado.

use crate::app::{App, CtxTab, Msg, Pop, Sec, View};
use iced::widget::text_editor;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;
use std::path::Path;

pub const SIZE: Size = Size::new(1440.0, 900.0);

pub fn logged() -> App {
    let mut a = App::new();
    a.logged = true;
    a.hour = Some(9);
    a
}

fn cena(f: impl FnOnce(&mut App)) -> App {
    let mut a = logged();
    f(&mut a);
    a.now = std::time::Instant::now() + std::time::Duration::from_secs(5); // animações já terminadas
    a
}

pub fn scenarios() -> Vec<(&'static str, Size, App)> {
    let mut v: Vec<(&'static str, Size, App)> = vec![("01-login", SIZE, { let mut a = App::new(); a.now = std::time::Instant::now(); a.login_at = a.now - std::time::Duration::from_secs(8); a })];
    let mut push = |n: &'static str, a: App| v.push((n, SIZE, a));
    push("01b-login-cifrado", { let mut a = App::new(); a.db_locked = true; a.now = std::time::Instant::now(); a.login_at = a.now - std::time::Duration::from_secs(8); a });
    push("02-inicio", cena(|a| {
        a.update(Msg::NewChat);
    }));
    push("03-chat-ram", cena(|a| {
        a.update(Msg::OpenConv(0));
    }));
    push("04-chat-ram-ferramentas", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::ToggleTools(0, 1));
    }));
    push("05-chat-rust", cena(|a| {
        a.update(Msg::OpenConv(1));
    }));
    push("06-chat-plano", cena(|a| {
        a.update(Msg::OpenConv(2));
    }));
    push("07-ctx-arquivos", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::CtxTab(CtxTab::Arquivos));
    }));
    push("08-ctx-diff", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::CtxTab(CtxTab::Diff));
    }));
    push("09-ctx-terminal", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::CtxTab(CtxTab::Terminal));
    }));
    for (n, vw) in [("10-projetos", View::Projetos), ("11-aprovacoes", View::Aprovacoes), ("12-rotinas", View::Rotinas), ("13-resultados", View::Resultados), ("14-nuvem", View::Nuvem), ("15-fontes", View::Fontes), ("16-conhecimento", View::Conhecimento), ("17-painel", View::Painel), ("18-vida", View::Vida), ("19-arquivadas", View::Arquivadas), ("20-memoria", View::Memoria)] {
        push(n, cena(|a| {
            a.more = !matches!(vw, View::Projetos | View::Aprovacoes | View::Rotinas | View::Memoria);
            a.update(Msg::Go(vw));
        }));
    }
    push("21-memoria-selecao", cena(|a| {
        a.update(Msg::Go(View::Memoria));
        let hub = (0..a.mem.nodes.len()).filter(|&i| a.mem.nodes[i].tipo == 0).max_by_key(|&i| a.mem.nodes[i].deg).unwrap();
        a.mem.sel = Some(hub);
        a.mem.cam = (a.mem.cam.0.max(1.1), -a.mem.rotp(hub).0, -a.mem.rotp(hub).1);
    }));
    for (n, sec) in [
        ("30-cfg-geral", Sec::Geral),
        ("31-cfg-notificacoes", Sec::Notificacoes),
        ("32-cfg-conta", Sec::Conta),
        ("33-cfg-privacidade", Sec::Privacidade),
        ("34-cfg-uso", Sec::Uso),
        ("35-cfg-modelos", Sec::Modelos),
        ("36-cfg-memoria", Sec::Memoria),
        ("37-cfg-fontes", Sec::Fontes),
        ("38-cfg-conectores", Sec::Conectores),
        ("39-cfg-sistema", Sec::Sistema),
        ("40-cfg-locais", Sec::Locais),
        ("41-cfg-voz", Sec::Voz),
        ("42-cfg-camera", Sec::Camera),
        ("43-cfg-atalhos", Sec::Atalhos),
    ] {
        push(n, cena(|a| {
            a.update(Msg::OpenConv(0));
            a.update(Msg::OpenSettings(Some(sec)));
            a.set_detail = false;
        }));
    }
    push("50-paleta", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::PalOpen);
    }));
    push("51-captura-rapida", cena(|a| {
        a.update(Msg::NewChat);
        a.update(Msg::QcapOpen);
    }));
    push("52-isso", cena(|a| {
        a.update(Msg::NewChat);
        a.update(Msg::IssoOpen);
    }));
    push("53-voz-ao-vivo", cena(|a| {
        a.update(Msg::NewChat);
        a.update(Msg::Live);
    }));
    push("54-editor", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::EditorOpen("03 Recursos/Orion/Memória/2026-10-15 - Orion RAM otimização.md".into()));
    }));
    push("55-menu-modelo", cena(|a| {
        a.update(Msg::NewChat);
        a.update(Msg::Pop(Pop::Model));
    }));
    push("56-menu-conta", cena(|a| {
        a.update(Msg::NewChat);
        a.update(Msg::Pop(Pop::Account));
    }));
    push("57-menu-conversa", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::Pop(Pop::ConvMenu));
    }));
    push("58-comandos", cena(|a| {
        a.update(Msg::NewChat);
        a.composer = text_editor::Content::with_text("/");
    }));
    push("59-aprovado", cena(|a| {
        a.update(Msg::OpenConv(0));
        a.update(Msg::Approve(0, 1));
    }));
    v.push(("60-gaveta", Size::new(700.0, 900.0), cena(|a| {
        a.win = Size::new(700.0, 900.0);
        a.update(Msg::OpenConv(0));
        a.drawer = true;
    })));
    // transições no meio do caminho (marcas de tempo + 100 ms)
    for (n, f) in [("70-anim-configuracoes", 0usize), ("71-anim-gaveta", 1), ("72-anim-pagina", 2)] {
        let mut a = logged();
        let size = if f == 1 { Size::new(700.0, 900.0) } else { SIZE };
        match f {
            0 => {
                a.update(Msg::OpenConv(0));
                a.update(Msg::OpenSettings(None));
            }
            1 => {
                a.win = size;
                a.update(Msg::OpenConv(0));
                a.update(Msg::Drawer(true));
            }
            _ => {
                a.update(Msg::Go(View::Projetos));
            }
        }
        let m = [a.at.modal, a.at.drawer, a.at.page][f].unwrap();
        a.now = m + std::time::Duration::from_millis(90);
        v.push((n, size, a));
    }
    v.push(("61-barra-recolhida", SIZE, cena(|a| {
        a.update(Msg::OpenConv(0));
        a.collapsed = true;
    })));
    v
}

pub fn render(app: &App, size: Size, out: &Path) {
    std::env::set_var("ICED_TEST_BACKEND", "tiny-skia");
    let settings = Settings { fonts: crate::theme::fonts(), default_font: crate::theme::UI, ..Settings::default() };
    let mut sim = Simulator::with_size(settings, size, app.view());
    let snap = sim.snapshot(&app.theme()).expect("snapshot");
    let _ = std::fs::remove_file(out);
    let _ = snap.matches_image(out);
}
