#![allow(unused_must_use)]
use iced::widget::text_editor;
use orion_app::app::{detect_kind, App, CapKind, Msg, Panic, Sec, View};
use orion_app::data::{ApprState, ChatMsg, ConvState};
use orion_app::md::{self, Block, Inline};
use orion_app::views::{overlays, settings};

fn logged() -> App {
    let mut a = App::new();
    a.update(Msg::PwChanged("x".into()));
    a.update(Msg::Login);
    a
}

#[test]
fn entrada_exige_senha_e_abre_o_chat_da_ram() {
    let mut a = App::new();
    a.update(Msg::Login);
    assert!(!a.logged);
    assert_eq!(a.gate_err, "Digite a senha para entrar.");
    a.update(Msg::PwChanged("qualquer".into()));
    a.update(Msg::Login);
    assert!(a.logged);
    assert_eq!(a.view, View::Chat);
    assert_eq!(a.convs[a.conv.unwrap()].id, "ram");
    assert_eq!(a.toast.as_ref().unwrap().0, "git pull do vault: 3 notas novas");
}

#[test]
fn pendencias_somam_conversas_e_proposta_da_rotina() {
    let a = logged();
    assert_eq!(a.pend_list().len(), 2);
    assert_eq!(a.pend_count(), 3);
}

#[test]
fn aprovar_resolve_a_pendencia_e_atualiza_o_contador() {
    let mut a = logged();
    a.update(Msg::Approve(0, 1));
    let ChatMsg::Orion(o) = &a.convs[0].msgs[1] else { panic!() };
    assert!(matches!(o.approval.as_ref().unwrap().state, ApprState::Approved(_)));
    assert_eq!(a.pend_count(), 2);
    a.update(Msg::Deny(2, 1));
    assert_eq!(a.convs[2].state, ConvState::Normal);
    assert_eq!(a.pend_count(), 1);
}

#[test]
fn painel_da_direita_abre_sozinho_com_aprovacao_pendente_e_some_em_janela_estreita() {
    let mut a = logged();
    assert!(a.show_ctx());
    a.update(Msg::Resized(iced::Size::new(1000.0, 800.0)));
    assert!(!a.show_ctx());
    a.update(Msg::Resized(iced::Size::new(1440.0, 900.0)));
    a.update(Msg::CtxToggle);
    assert!(!a.show_ctx());
}

#[test]
fn navegacao_empilha_voltar_e_avancar() {
    let mut a = logged();
    a.update(Msg::Go(View::Projetos));
    a.update(Msg::Go(View::Rotinas));
    a.update(Msg::Back);
    assert_eq!(a.view, View::Projetos);
    a.update(Msg::Fwd);
    assert_eq!(a.view, View::Rotinas);
    a.update(Msg::Back);
    a.update(Msg::Go(View::Conhecimento));
    assert!(a.fwd.is_empty());
}

#[test]
fn abrir_rotinas_apaga_o_resultado_novo() {
    let mut a = logged();
    assert!(a.rots.iter().any(|r| r.news));
    a.update(Msg::Go(View::Rotinas));
    assert!(a.rots.iter().all(|r| !r.news));
}

#[test]
fn enviar_cria_conversa_nova_no_inicio_e_transmite_a_resposta() {
    let mut a = logged();
    a.update(Msg::NewChat);
    a.update(Msg::Fill("Oi, Orion"));
    a.update(Msg::Send);
    assert_eq!(a.view, View::Chat);
    assert_eq!(a.convs[0].title, "Oi, Orion");
    assert!(a.streaming.is_some());
    for _ in 0..200 {
        a.update(Msg::Tick(std::time::Instant::now()));
    }
    assert!(a.streaming.is_none());
    assert_eq!(a.convs[0].state, ConvState::Normal);
}

#[test]
fn comando_desconhecido_avisa_e_ajuda_lista() {
    let mut a = logged();
    a.update(Msg::NewChat);
    a.update(Msg::Fill("/xyz"));
    a.update(Msg::Send);
    assert!(a.toast.as_ref().unwrap().0.contains("não existe"));
    a.update(Msg::Fill("/aprovar"));
    a.update(Msg::Send);
    assert_eq!(a.view, View::Aprovacoes);
}

#[test]
fn captura_rapida_detecta_o_tipo() {
    assert_eq!(detect_kind("gastei 12,50 no almoço"), CapKind::Gasto);
    assert_eq!(detect_kind("pagar R$ 30"), CapKind::Gasto);
    assert_eq!(detect_kind("lembra de ligar amanhã"), CapKind::Lembrete);
    assert_eq!(detect_kind("comprar café"), CapKind::Tarefa);
    assert_eq!(detect_kind("ideia solta"), CapKind::Nota);
}

#[test]
fn captura_rapida_salva_no_historico() {
    let mut a = logged();
    a.update(Msg::QcapOpen);
    a.update(Msg::QcapText("comprar pão".into()));
    a.update(Msg::QcapSave);
    assert_eq!(a.acts[0].text, "Captura rápida (tarefa): comprar pão");
    assert!(a.toast.as_ref().unwrap().0.starts_with("Tarefa salva em 00 Inbox"));
}

#[test]
fn desfazer_marca_o_original_e_registra_a_reversao() {
    let mut a = logged();
    a.update(Msg::Undo(0));
    assert!(a.acts[1].undone);
    assert!(a.acts[0].text.starts_with("Desfeito: Nota gravada"));
    assert_eq!(a.toast.as_ref().unwrap().0, "git revert feito e push na main do vault");
}

#[test]
fn pausar_tudo_confirma_em_dois_passos_e_desliga_o_autonomo() {
    let mut a = logged();
    a.update(Msg::Toggle("auto"));
    assert!(!a.p.auto);
    assert!(a.auto_senha.is_some());
    a.update(Msg::AutoSenhaOk);
    assert!(!a.p.auto, "senha vazia não confirma o modo autônomo");
    a.update(Msg::AutoSenhaText("senha-da-simulacao".into()));
    a.update(Msg::AutoSenhaOk);
    assert!(a.p.auto);
    a.update(Msg::PanicArm);
    assert_eq!(a.panic, Panic::Armed);
    a.update(Msg::PanicConfirm);
    assert_eq!(a.panic, Panic::Paused);
    assert!(!a.p.auto);
    a.update(Msg::PanicResume);
    assert_eq!(a.panic, Panic::Idle);
}

#[test]
fn apagar_historico_zera_conversas_e_volta_ao_inicio() {
    let mut a = logged();
    a.update(Msg::OpenSettings(Some(Sec::Privacidade)));
    a.update(Msg::WipeArm);
    a.update(Msg::WipeConfirm);
    assert!(a.convs.is_empty());
    assert_eq!(a.view, View::Home);
    assert!(!a.set_open);
}

#[test]
fn busca_das_configuracoes_olha_nome_e_palavras_chave() {
    assert_eq!(settings::search("palmas"), vec![Sec::Voz]);
    assert!(settings::search("mcp").contains(&Sec::Conectores));
    assert!(settings::search("xyzxyz").is_empty());
    assert_eq!(settings::search("").len(), 16);
}

#[test]
fn paleta_ordena_conversas_notas_e_acoes() {
    let mut a = logged();
    a.update(Msg::PalOpen);
    let all = overlays::pal_items(&a);
    assert!(all.iter().take(6).all(|i| i.group.starts_with("Conversa")));
    assert!(all.iter().any(|i| i.title == "Nova conversa"));
    a.update(Msg::PalQ("orion".into()));
    let q = overlays::pal_items(&a);
    assert!(q.iter().any(|i| i.group.starts_with("Nota ·")));
}

#[test]
fn escala_e_densidade_vem_das_configuracoes() {
    let mut a = logged();
    a.update(Msg::Pick("scale", 4));
    assert_eq!(a.p.scale, 150);
    a.update(Msg::Pick("density", 1));
    assert!(a.p.compact);
}

#[test]
fn arquivar_tira_da_lista_e_vai_para_arquivadas() {
    let mut a = logged();
    let n = a.convs.len();
    a.update(Msg::ArchiveConv);
    assert_eq!(a.convs.len(), n - 1);
    assert_eq!(a.view, View::Home);
    assert_eq!(a.archived.len(), 3);
}

#[test]
fn markdown_reconhece_blocos_e_trechos() {
    let b = md::parse("---\ntype: x\n---\n\n# Título\n\n> cit [[Orion]]\n\n- a\n- b\n\n1. um\n2. dois\n\n```rust\nlet x = 1;\n```\n\nTexto **forte** e `código`.");
    assert!(matches!(b[0], Block::Front(_)));
    assert!(matches!(b[1], Block::H(1, _)));
    assert!(matches!(b[2], Block::Quote(_)));
    assert!(matches!(&b[3], Block::Ul(v) if v.len() == 2));
    assert!(matches!(&b[4], Block::Ol(v) if v.len() == 2));
    assert!(matches!(&b[5], Block::Code(_, v) if v.len() == 1));
    let Block::P(p) = &b[6] else { panic!() };
    assert!(p.contains(&Inline::Bold("forte".into())) && p.contains(&Inline::Code("código".into())));
    let Block::Quote(q) = &b[2] else { panic!() };
    assert!(q.contains(&Inline::Wiki("Orion".into())));
}

#[test]
fn wikilink_com_rotulo_mostra_o_rotulo() {
    assert_eq!(md::inline("[[alvo|rótulo]]"), vec![Inline::Wiki("rótulo".into())]);
}

#[test]
fn grafo_carrega_o_snapshot_e_respeita_filtros_e_data() {
    let mut a = logged();
    a.update(Msg::Go(View::Memoria));
    let g = &a.mem;
    assert_eq!(g.nodes.len(), 895);
    assert_eq!(g.links.len(), 3804);
    assert!(!g.filt[6], "relatórios começam desligados");
    let (n0, _) = g.stats();
    a.update(Msg::MemFilter(6));
    assert!(a.mem.stats().0 > n0);
    a.update(Msg::MemCut(0.0));
    assert!(a.mem.stats().0 < n0 + 200);
    assert!(a.mem.nodes.iter().any(|n| n.hub));
}

#[test]
fn zoom_no_ponto_mantem_o_ponto_sob_o_cursor() {
    let mut a = logged();
    a.update(Msg::Go(View::Memoria));
    a.mem.rot = 0.0;
    let (w, h) = orion_app::views::memory::stage_size(&a);
    let before = a.mem.to_world(300.0, 200.0, w, h);
    a.update(Msg::MemZoomAt(300.0, 200.0, 1.12));
    let after = a.mem.to_world(300.0, 200.0, w, h);
    assert!((before.0 - after.0).abs() < 0.01 && (before.1 - after.1).abs() < 0.01);
}

#[test]
fn editor_abre_e_salva_com_toast() {
    let mut a = logged();
    a.update(Msg::EditorOpen("03 Recursos/Orion/Nota.md".into()));
    assert_eq!(a.editor.as_ref().unwrap().name, "Nota");
    a.update(Msg::EditorEdit(text_editor::Action::Move(text_editor::Motion::DocumentEnd)));
    a.update(Msg::EditorSave);
    assert!(a.editor.is_none());
    assert_eq!(a.toast.as_ref().unwrap().0, "Nota salva; commit e push na main do vault");
}

#[test]
fn sugestoes_de_barra_e_arroba() {
    let mut a = logged();
    a.update(Msg::NewChat);
    a.update(Msg::Fill("/vau"));
    let s = orion_app::views::composer::suggestions(&a);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].1, "/vault");
    a.update(Msg::Fill("veja @Orion"));
    let s = orion_app::views::composer::suggestions(&a);
    assert!(!s.is_empty() && s.len() <= 8);
    assert!(s[0].0.starts_with("@[["));
}

#[test]
fn curva_ease_vai_de_zero_a_um_e_sai_rapido() {
    use orion_app::anim::{ease, progress};
    use std::time::{Duration, Instant};
    assert_eq!(ease(0.0), 0.0);
    assert_eq!(ease(1.0), 1.0);
    assert!(ease(0.25) > 0.6, "cubic-bezier(.16,1,.3,1) arranca rápido");
    let t0 = Instant::now();
    assert_eq!(progress(None, t0, 220, false), 1.0);
    assert_eq!(progress(Some(t0), t0, 220, true), 1.0, "movimento reduzido não anima");
    assert!(progress(Some(t0), t0 + Duration::from_millis(110), 220, false) > 0.5);
    assert_eq!(progress(Some(t0), t0 + Duration::from_millis(300), 220, false), 1.0);
}

#[test]
fn transicoes_marcam_o_tempo_so_quando_algo_muda() {
    let mut a = logged();
    assert!(a.at.page.is_some());
    a.at.page = None;
    a.update(Msg::Go(View::Projetos));
    assert!(a.at.page.is_some());
    a.update(Msg::OpenSettings(None));
    assert!(a.at.modal.is_some() && a.at.scrim.is_some());
    a.update(Msg::Toggle("dnd"));
    assert!(a.at.switch.as_ref().unwrap().0.contains("dnd"));
    a.update(Msg::ToggleMore);
    assert!(a.at.more.is_some());
}

#[test]
fn tracejado_tooltips_e_hover_nao_quebram_a_renderizacao() {
    // monta a árvore de várias telas (os widgets novos entram em todas)
    let mut a = logged();
    a.update(Msg::Go(View::Memoria));
    let _ = a.view();
    // Vida saiu do app; mantém a verificação numa página operacional da v1.
    a.update(Msg::Go(View::Projetos));
    let _ = a.view();
    a.update(Msg::OpenConv(0));
    a.update(Msg::Pop(orion_app::app::Pop::Model));
    let _ = a.view();
}
