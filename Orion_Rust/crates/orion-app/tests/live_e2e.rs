//! O app inteiro contra um servidor de verdade: as mensagens passam por `App::update`, as tarefas rodam e o resultado volta.
//! Só roda com `ORION_TEST_URL` e `ORION_TEST_SENHA` (o servidor precisa de um modelo; o teste usa um modelo falso).
use futures::StreamExt;
use iced_runtime::Action;
use orion_app::app::{App, Msg, Sec, View};
use orion_app::data::{ChatMsg, ConvState};
use orion_app::net::Api;

/// Entrega a mensagem e roda, em cadeia, tudo que as tarefas devolverem.
async fn drive(app: &mut App, msg: Msg) {
    let mut fila = vec![msg];
    let mut passos = 0;
    while let Some(m) = fila.pop() {
        passos += 1;
        assert!(passos < 400, "laço de mensagens não terminou");
        let task = app.update(m);
        if let Some(mut s) = iced_runtime::task::into_stream(task) {
            while let Some(a) = s.next().await {
                if let Action::Output(m) = a {
                    fila.insert(0, m);
                }
            }
        }
    }
}

#[tokio::test]
async fn entrar_conversar_fixar_arquivar_e_sair() {
    let (Ok(url), Ok(senha)) = (std::env::var("ORION_TEST_URL"), std::env::var("ORION_TEST_SENHA")) else { return };
    let (mut app, boot) = App::live(Api::new(&url, None));
    // abertura: servidor no ar, ainda sem login
    let mut s = iced_runtime::task::into_stream(boot).unwrap();
    while let Some(a) = s.next().await {
        if let Action::Output(m) = a {
            drive(&mut app, m).await;
        }
    }
    assert!(!app.logged);
    assert!(app.gate_err.is_empty(), "{}", app.gate_err);

    // senha errada
    drive(&mut app, Msg::PwChanged("errada".into())).await;
    drive(&mut app, Msg::Login).await;
    assert!(!app.logged);
    assert_eq!(app.gate_err, "Usuário ou senha incorretos.");

    // senha certa → entra e carrega as conversas do servidor
    drive(&mut app, Msg::PwChanged(senha)).await;
    drive(&mut app, Msg::Login).await;
    assert!(app.logged, "{}", app.gate_err);
    assert!(app.convs.iter().all(|c| !c.id.is_empty()));

    // conversa nova pelo Início: cria a sessão, manda o texto e recebe a resposta pelo SSE
    let antes = app.convs.len();
    drive(&mut app, Msg::NewChat).await;
    drive(&mut app, Msg::Fill("oi, Orion")).await;
    drive(&mut app, Msg::Send).await;
    assert_eq!(app.view, View::Chat);
    assert_eq!(app.convs.len(), antes + 1, "a conversa nova aparece na lista");
    let ci = app.conv.unwrap();
    let c = &app.convs[ci];
    assert_eq!(c.state, ConvState::Normal);
    assert!(app.streaming.is_none());
    let ChatMsg::Orion(o) = c.msgs.last().unwrap() else { panic!("sem resposta") };
    assert!(o.body.contains("modelo de teste"), "corpo: {:?}", o.body);
    assert!(o.saved);
    let sid = c.id.clone();

    // reabrir lê o histórico do servidor
    drive(&mut app, Msg::Go(View::Projetos)).await;
    let i = app.convs.iter().position(|c| c.id == sid).unwrap();
    drive(&mut app, Msg::OpenConv(i)).await;
    let c = &app.convs[app.conv.unwrap()];
    assert_eq!(c.id, sid);
    assert!(matches!(&c.msgs[0], ChatMsg::User(t) if t == "oi, Orion"));
    assert_eq!(c.msgs.len(), 2);

    // fixar persiste no servidor
    drive(&mut app, Msg::PinToggle).await;
    assert!(app.convs.iter().find(|c| c.id == sid).unwrap().pinned);

    // as páginas carregam do servidor sem erro
    for v in [View::Rotinas, View::Conhecimento, View::Aprovacoes, View::Memoria, View::Projetos] {
        app.toast = None;
        drive(&mut app, Msg::Go(v)).await;
        assert!(app.toast.is_none(), "{v:?}: {:?}", app.toast);
        assert!(app.logged);
    }

    // O painel agora carrega pela seção das configurações.
    drive(&mut app, Msg::OpenSettings(Some(Sec::Painel))).await;
    assert!(app.painel.is_some());
    assert!(app.budget.is_some());
    drive(&mut app, Msg::CloseSettings).await;

    // arquivar tira da lista; apagar a arquivada também funciona no servidor
    let i = app.convs.iter().position(|c| c.id == sid).unwrap();
    drive(&mut app, Msg::OpenConv(i)).await;
    drive(&mut app, Msg::ArchiveConv).await;
    assert!(app.convs.iter().all(|c| c.id != sid));

    // sair derruba a sessão: a próxima leitura volta para a entrada
    drive(&mut app, Msg::Logout).await;
    assert!(!app.logged);
}

async fn entrar(url: &str, senha: &str) -> App {
    let (mut app, boot) = App::live(Api::new(url, None));
    let mut s = iced_runtime::task::into_stream(boot).unwrap();
    while let Some(a) = s.next().await {
        if let Action::Output(m) = a {
            drive(&mut app, m).await;
        }
    }
    drive(&mut app, Msg::PwChanged(senha.into())).await;
    drive(&mut app, Msg::Login).await;
    assert!(app.logged, "{}", app.gate_err);
    app
}

#[tokio::test]
async fn aprovar_e_negar_pelo_app() {
    let (Ok(url), Ok(senha)) = (std::env::var("ORION_TEST_URL"), std::env::var("ORION_TEST_SENHA")) else { return };
    let _ = std::fs::remove_file("/root/orion-teste.txt");
    let mut app = entrar(&url, &senha).await;

    // o modelo pede para escrever um arquivo fora das pastas seguras: o servidor pede aprovação
    drive(&mut app, Msg::NewChat).await;
    drive(&mut app, Msg::Fill("ESCREVER um arquivo")).await;
    drive(&mut app, Msg::Send).await;
    let ci = app.conv.unwrap();
    assert_eq!(app.convs[ci].state, ConvState::Pending, "a conversa fica aguardando você");
    let pend = app.pend_list();
    assert!(pend.iter().any(|(c, _)| *c == ci), "a pendência aparece em Aprovações");
    let (c, m) = *pend.iter().find(|(c, _)| *c == ci).unwrap();
    let ChatMsg::Orion(o) = &app.convs[c].msgs[m] else { panic!() };
    let a = o.approval.as_ref().unwrap();
    assert!(!a.id.is_empty() && a.text.contains("escrever_arquivo"), "texto: {}", a.text);
    assert!(!std::path::Path::new("/root/orion-teste.txt").exists(), "nada foi escrito antes de aprovar");

    // o sondeio do servidor enxerga a mesma pendência (e não duplica)
    drive(&mut app, Msg::Poll).await;
    assert_eq!(app.pend_list().iter().filter(|(c, _)| *c == ci).count(), 1);

    // aprovar: decide, retoma o turno e o modelo conclui
    drive(&mut app, Msg::Approve(c, m)).await;
    assert_eq!(app.convs[ci].state, ConvState::Normal);
    let ChatMsg::Orion(ultima) = app.convs[ci].msgs.last().unwrap() else { panic!() };
    assert!(!ultima.body.is_empty() && ultima.tools.iter().any(|t| t.value.contains("escrever_arquivo")), "a retomada trouxe a ferramenta e o texto: {ultima:?}");
    let ChatMsg::Orion(pedido) = &app.convs[ci].msgs[m] else { panic!() };
    assert!(matches!(pedido.approval.as_ref().unwrap().state, orion_app::data::ApprState::Approved(_)));
    assert!(app.pend_list().iter().all(|(c, _)| *c != ci));

    // negar: nada é escrito
    let _ = std::fs::remove_file("/root/orion-teste.txt");
    drive(&mut app, Msg::NewChat).await;
    drive(&mut app, Msg::Fill("ESCREVER de novo")).await;
    drive(&mut app, Msg::Send).await;
    let ci2 = app.conv.unwrap();
    let (c, m) = *app.pend_list().iter().find(|(c, _)| *c == ci2).unwrap();
    drive(&mut app, Msg::Deny(c, m)).await;
    assert!(!std::path::Path::new("/root/orion-teste.txt").exists());
    assert!(app.pend_list().iter().all(|(c, _)| *c != ci2));
    let ChatMsg::Orion(neg) = &app.convs[ci2].msgs[m] else { panic!() };
    assert!(matches!(neg.approval.as_ref().unwrap().state, orion_app::data::ApprState::Denied(_)));
    drive(&mut app, Msg::Logout).await;
}
