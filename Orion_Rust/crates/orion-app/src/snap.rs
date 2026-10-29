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
    for (i, name) in ["00-onboarding-1-conta", "00-onboarding-2-seguranca", "00-onboarding-3-notas", "00-onboarding-4-chaves", "00-onboarding-5-telegram-backup", "00-onboarding-6-importar", "00-onboarding-7-pronto"].into_iter().enumerate() {
        let mut a = App::new();
        let mut f = crate::onboarding::Form::new(serde_json::json!({"user":"antoniosalomao","encrypted_available":true,"hello_available":true,"vault":"C:\\Vault","backup":"D:\\Backups","detected":[{"source":"Claude","kind":"mcp","path":"C:\\Users\\a\\.claude.json"}]}));
        f.step = i;
        f.step_at = std::time::Instant::now() - std::time::Duration::from_secs(5);
        f.name = "Antônio".into();
        f.settle_sky(i, SIZE);
        if i == 6 {
            f.code = Some("K7QM-4XNA-9PD2-HW8T-R3VC-5ZEB".into());
        }
        if i == 3 {
            f.tests.insert("ORION_KEY_GEMINI".into(), "Conexão verificada".into());
            f.secrets.insert("ORION_KEY_GEMINI".into(), "chave".into());
        }
        a.onboarding = Some(f);
        a.now = std::time::Instant::now();
        v.push((name, SIZE, a));
    }
    let mut push = |n: &'static str, a: App| v.push((n, SIZE, a));
    push("01b-login-cifrado", { let mut a = App::new(); a.db_locked = true; a.now = std::time::Instant::now(); a.login_at = a.now - std::time::Duration::from_secs(8); a });
    {
        use crate::island::{Phase, Purpose};
        use crate::net::{FaceHint, FaceFailure};
        let island_scene = |phase: Phase, age_ms: u64, purpose: Purpose, logged_in: bool| {
            let mut a = App::new();
            a.user = "antonio".into();
            a.island.demo = true;
            if logged_in { a.logged = true; a.hour = Some(9); a.update(Msg::NewChat); }
            a.now = std::time::Instant::now() + std::time::Duration::from_secs(10);
            a.login_at = a.now - std::time::Duration::from_secs(8);
            a.island.phase = phase;
            a.island.purpose = purpose;
            a.island.since = a.now - std::time::Duration::from_millis(age_ms);
            a.island.from = crate::island::size(phase);
            a.island.hint = FaceHint::Reading;
            if phase == Phase::Error { a.island.failure = Some(FaceFailure::NotRecognized); }
            a
        };
        push("40-faceid-espera", island_scene(Phase::Standby, 4000, Purpose::Login, false));
        push("41-faceid-lendo", island_scene(Phase::Reading, 1300, Purpose::Login, false));
        push("42-faceid-sucesso", island_scene(Phase::Success, 620, Purpose::Login, false));
        push("43-faceid-erro", island_scene(Phase::Error, 900, Purpose::Login, false));
        push("44-faceid-autonomo", { let mut a = island_scene(Phase::Reading, 1300, Purpose::Autonomous, true); a.auto_senha = Some(String::new()); a });
    }
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
    for (n, vw) in [("10-projetos", View::Projetos), ("11-aprovacoes", View::Aprovacoes), ("12-rotinas", View::Rotinas), ("14-nuvem", View::Nuvem), ("16-conhecimento", View::Conhecimento), ("19-arquivadas", View::Arquivadas), ("20-memoria", View::Memoria)] {
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
        ("17-cfg-painel", Sec::Painel),
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
    // Telas das pendências da v1 (diferentes das do protótipo: só existem aqui).
    push("80-pensando-ferramentas", cena(|a| {
        a.update(Msg::OpenConv(0));
        streaming_com(a, vec![("1", "Ler arquivo", "orion-core/src/agent.rs", "ok"), ("2", "Executar comando", "cargo test -p orion-core", "rodando")], "");
    }));
    push("81-pensando-vazio", cena(|a| {
        a.update(Msg::OpenConv(0));
        streaming_com(a, vec![], "");
    }));
    push("82-anexo-imagem", cena(|a| {
        a.update(Msg::OpenConv(0));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(64, 64, |x, y| image::Rgba([(x * 4) as u8, (y * 4) as u8, 180, 255]))).write_to(&mut png, image::ImageFormat::Png).unwrap();
        a.attach = vec![("captura.png".into(), "imagem".into()), ("relatorio.pdf".into(), ".pdf".into())];
        a.attachment_ids = vec!["img1".into(), "doc1".into()];
        a.attach_thumbs.insert("img1".into(), iced::widget::image::Handle::from_bytes(png.into_inner()));
    }));
    push("83-conflito-vault", cena(|a| {
        let original = "---\ntype: conversa\ndate: 2026-10-27\nfonte: orion\n---\n\n# Conversa\n\nTexto que você editou no Obsidian.\n".to_string();
        let proposal = "---\ntype: conversa\ndate: 2026-10-27\nfonte: orion\n---\n\n# Conversa\n\n**Antônio:** oi\n\n**Orion:** olá, como posso ajudar?\n".to_string();
        a.conflito = Some(crate::app::ConflitoState { proposed_path: "03 Recursos/Orion/Memória/x - conflito Orion 0123456789abcdef0123.md".into(), original_path: "03 Recursos/Orion/Memória/x.md".into(), content: text_editor::Content::with_text(&original), original, proposal, sha: "0".into(), busy: false });
    }));
    push("84-projeto-novo", cena(|a| {
        a.update(Msg::Go(View::Projetos));
        a.project_form = Some(crate::app::ProjectForm::default());
        if let Some(p) = a.projects.first_mut() { p.vault = true; }
    }));
    push("85-config-memoria-conflitos", cena(|a| {
        a.conflitos = vec![("03 Recursos/Orion/Memória/2026-10-27 - orion - Conversa.md".into(), "x".into(), true)];
        a.update(Msg::OpenSettings(Some(Sec::Memoria)));
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
    render_scrolled(app, size, out, 0.0);
}

/// Captura também os trechos abaixo da dobra de uma tela rolável.
pub fn render_scrolled(app: &App, size: Size, out: &Path, offset: f32) {
    std::env::set_var("ICED_TEST_BACKEND", "tiny-skia");
    let settings = Settings { fonts: crate::theme::fonts(), default_font: crate::theme::UI, ..Settings::default() };
    let mut sim = Simulator::with_size(settings, size, app.view());
    sim.point_at(iced::Point::new(size.width / 2.0, size.height / 2.0));
    sim.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -offset },
    })]);
    let snap = sim.snapshot(&app.theme()).expect("snapshot");
    let _ = std::fs::remove_file(out);
    let _ = snap.matches_image(out);
}

/// Conversa aberta respondendo: a última fala do Orion com as ferramentas dadas (id, nome, resumo, estado).
fn streaming_com(a: &mut App, tools: Vec<(&str, &str, &str, &str)>, body: &str) {
    use crate::data::{ChatMsg, ConvState, OrionMsg, Tool};
    let Some(ci) = a.conv else { return };
    a.convs[ci].msgs.push(ChatMsg::Orion(OrionMsg {
        time: "agora".into(), via: String::new(), out: String::new(), tools_label: String::new(),
        tools: tools.into_iter().map(|(id, kind, value, time)| Tool { id: id.into(), kind: kind.into(), value: value.into(), time: time.into() }).collect(),
        body: body.into(), saved: false, approval: None,
    }));
    a.convs[ci].state = ConvState::Running;
    a.streaming = Some(crate::app::Stream { conv: ci, words: 0, total: usize::MAX });
}
