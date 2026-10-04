/* ==========================================================================
   ORION — app.js | rotas por hash, atalhos, janela, boot e ligação dos módulos
   Rotas: #/ · #/chat · #/memoria · #/integracoes · #/config (botão voltar funciona,
   dá para abrir direto numa tela). Telas ocultas ficam `inert`: nada de Tab invisível.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, bus, prefs, ui, api } = O;
    const U = O.util;
    const html = document.documentElement;
    O.versao = '2.0.0';

    /* ── rotas ─────────────────────────────────────────────────────────── */
    const VIEWS = {
        home: { titulo: 'Início', rota: '/' },
        chat: { titulo: 'Chat', rota: '/chat' },
        memoria: { titulo: 'Memória', rota: '/memoria' },
        integracoes: { titulo: 'Integrações', rota: '/integracoes' },
        config: { titulo: 'Configurações', rota: '/config' },
    };
    const VIEW_DA_ROTA = Object.fromEntries(Object.entries(VIEWS).map(([v, d]) => [d.rota, v]));
    let atual = null, opcoesPendentes = {};

    const viewDaUrl = () => {
        const bruto = location.hash.replace(/^#/, '').split('?')[0].replace(/\/+$/, '') || '/';
        return VIEW_DA_ROTA[bruto] || 'home';
    };

    function ir(view, opcoes = {}) {
        if (!VIEWS[view]) view = 'home';
        opcoesPendentes = opcoes;
        const alvo = `#${VIEWS[view].rota}`;
        const igual = view === 'home' ? (location.hash === '' || location.hash === '#/' || location.hash === '#') : location.hash === alvo;
        if (igual) mostrar(view, opcoes, false);
        else location.hash = alvo;      // dispara hashchange → mostrar()
    }

    function subtituloDoChat() {
        const s = O.sidebar?.ativa?.();
        $('#tb-sub').textContent = atual === 'chat' && s ? s.titulo || '' : '';
    }

    function mostrar(view, opcoes = {}, inicial = false) {
        const anterior = atual;
        if (anterior && anterior !== view) O.views[anterior]?.desativar?.();
        atual = view;
        html.dataset.view = view;
        if (anterior === 'chat' && view !== 'chat' && O.chat.pendentes().length) $('#badge-chat').classList.add('show');
        for (const v of $$('.view')) {
            const on = v.dataset.view === view;
            v.dataset.active = String(on);
            v.inert = !on;
        }
        for (const a of $$('.sb-item')) {
            if (a.dataset.view === view) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
        }
        $('#tb-title').textContent = VIEWS[view].titulo;
        document.title = view === 'home' ? 'Orion' : `${VIEWS[view].titulo} · Orion`;
        subtituloDoChat();
        O.sky.setModo(view === 'home' ? 'home' : 'fundo');
        if (view === 'chat') { $('#badge-chat').classList.remove('show'); O.chat.irAoFim(); }
        O.views[view]?.ativar?.(opcoes);
        if (!inicial && opcoes.foco !== false) {
            if (view === 'chat' || view === 'home') O.composer.foco();
            else $('#main').focus({ preventScroll: true });
        }
        bus.emit('view', view);
    }

    /* ── ações compartilhadas (paleta, atalhos, botões) ────────────────── */
    const NOMES_TEMA = { noite: 'Noite', grafite: 'Grafite', contraste: 'Alto contraste' };
    O.acoes = {
        alternarTts() {
            if (!api.suporta('tts')) return ui.toast('Resposta por voz ainda indisponível neste backend.', { tipo: 'aviso' });
            const mudo = !prefs.get('tts_mudo');
            prefs.set('tts_mudo', mudo);
            api.ttsMudo(mudo).catch(() => ui.toast('Resposta por voz indisponível neste backend.', { tipo: 'aviso' }));
            O.som.envio();
            O.anunciar(mudo ? 'Resposta por voz desligada.' : 'Resposta por voz ligada.');
        },
        tema(t) { prefs.set('theme', t); ui.toast(`Tema ${NOMES_TEMA[t]}.`, { ms: 1800 }); },
        densidade(d) { prefs.set('density', d); ui.toast(`Densidade ${d === 'compacta' ? 'compacta' : 'confortável'}.`, { ms: 1800 }); },
        escala(delta) { prefs.set('scale', U.clamp(Math.round((prefs.get('scale') + delta) * 100) / 100, 0.9, 1.3)); },
        async exportar() {
            try {
                const d = await api.exportar();
                if (!d.markdown || d.total_msgs === 0) { ui.toast('Nada para exportar ainda.', { tipo: 'aviso' }); return; }
                const url = URL.createObjectURL(new Blob([d.markdown], { type: 'text/markdown;charset=utf-8' }));
                const a = document.createElement('a');
                a.href = url;
                a.download = `orion-conversa-${new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-')}.md`;
                document.body.append(a); a.click(); a.remove();
                setTimeout(() => URL.revokeObjectURL(url), 2000);
                ui.toast(`Conversa exportada (${d.total_msgs} mensagens).`, { tipo: 'ok' });
            } catch (e) { ui.toast(`Falha ao exportar: ${e.message}`, { tipo: 'erro' }); }
        },
        async limpar() {
            if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar, ou pare com Esc, para limpar.', { tipo: 'aviso', ms: 2800, id: 'ocupado' }); return; }
            const sim = await ui.confirmar({ titulo: 'Limpar o histórico desta sessão?', ok: 'Limpar', perigo: true,
                texto: 'Apaga a conversa em memória nesta sessão. A memória de longo prazo não é afetada.' });
            if (!sim || O.chat.ocupado()) return;
            try { await api.limparHistorico(); O.chat.limpar(); ui.toast('Histórico em memória limpo.', { tipo: 'ok' }); }
            catch (e) { ui.toast(`Falha ao limpar: ${e.message}`, { tipo: 'erro' }); }
        },
    };

    Object.assign(O.acoes, {
        foco() {
            const ligar = html.dataset.foco !== 'true';
            if (ligar) html.dataset.foco = 'true'; else delete html.dataset.foco;
            O.sky.resize();
            if (ligar) ui.toast('Modo foco. Ctrl+. ou Esc para sair.', { ms: 2600, id: 'foco' });
        },
        async copiarUltima() {
            const t = O.chat.ultimaResposta();
            if (!t) { ui.toast('Ainda não há resposta para copiar.', { tipo: 'aviso', ms: 2400 }); return; }
            ui.toast((await ui.copiar(t)) ? 'Resposta copiada.' : 'Não consegui copiar.', { tipo: 'ok', ms: 1800 });
        },
        async copiarConversa() {
            try {
                const d = await api.exportar();
                if (!d.markdown || !d.total_msgs) { ui.toast('Nada para copiar ainda.', { tipo: 'aviso', ms: 2400 }); return; }
                ui.toast((await ui.copiar(d.markdown)) ? `Conversa copiada (${d.total_msgs} mensagens).` : 'Não consegui copiar.', { tipo: 'ok', ms: 2000 });
            } catch (e) { ui.toast(`Falha ao copiar: ${e.message}`, { tipo: 'erro' }); }
        },
        modelo(id) {
            if (!api.suporta('model_selection')) return ui.toast('A seleção de modelo ainda está indisponível neste backend.', { tipo: 'aviso' });
            prefs.set('model', id);
            const m = O.composer.MODELOS.find(x => x.id === id);
            ui.toast(`Modelo: ${m ? m.nome : id}.`, { ms: 1800 });
        },
        /** executa o que o campo de mensagem interpretou como `/comando` (ver slash.js) */
        slash(cmd, arg) {
            const A = O.acoes;
            const mapa = {
                nova: () => O.sidebar.nova(), buscar: () => O.busca.abrir(arg),
                copiar: () => (arg === 'conversa' ? A.copiarConversa() : A.copiarUltima()),
                exportar: () => A.exportar(), limpar: () => A.limpar(), modelo: () => A.modelo(arg), tema: () => A.tema(arg),
                foco: () => A.foco(), mudo: () => A.alternarTts(), voz: () => O.voz.alternar(),
                inicio: () => ir('home'), memoria: () => ir('memoria'), integracoes: () => ir('integracoes'), config: () => ir('config'),
                ajuda: () => ir('config', { secao: 'cfg-atalhos' }),
            };
            mapa[cmd]?.();
        },
    });

    /* ── atalhos: uma tabela só serve ao teclado, à paleta e às Configurações ── */
    const ctrl = e => e.ctrlKey || e.metaKey;
    const ATALHOS = [
        { grupo: 'Geral', rotulo: 'Paleta de comandos', teclas: ['Ctrl', 'K'], quando: e => ctrl(e) && !e.shiftKey && e.key.toLowerCase() === 'k', fn: () => O.palette.alternar() },
        { grupo: 'Geral', rotulo: 'Nova conversa', teclas: ['Ctrl', '⇧', 'O'], quando: e => ctrl(e) && e.shiftKey && e.key.toLowerCase() === 'o', fn: () => O.sidebar.nova() },
        { grupo: 'Geral', rotulo: 'Recolher barra lateral', teclas: ['Ctrl', 'B'], quando: e => ctrl(e) && !e.shiftKey && e.key.toLowerCase() === 'b', fn: () => O.sidebar.alternar() },
        { grupo: 'Geral', rotulo: 'Mostrar atalhos', teclas: ['?'], digitando: false, quando: e => e.key === '?' && !ctrl(e), fn: () => ir('config', { secao: 'cfg-atalhos' }) },
        { grupo: 'Geral', rotulo: 'Modo foco (sem barras)', teclas: ['Ctrl', '.'], quando: e => ctrl(e) && !e.shiftKey && e.key === '.', fn: () => O.acoes.foco() },
        { grupo: 'Geral', rotulo: 'Fechar painel ou voltar ao início', teclas: ['Esc'] },
        ...Object.keys(VIEWS).map((v, i) => ({ grupo: 'Navegação', rotulo: VIEWS[v].titulo, teclas: ['Alt', String(i + 1)],
            quando: e => e.altKey && !ctrl(e) && !e.shiftKey && e.code === `Digit${i + 1}`, fn: () => ir(v) })),
        { grupo: 'Chat', rotulo: 'Focar na caixa de mensagem', teclas: ['/'], digitando: false, quando: e => e.key === '/' && !ctrl(e), fn: () => { if (atual !== 'home') ir('chat'); else O.composer.foco(); } },
        { grupo: 'Chat', rotulo: 'Comandos (digite / no começo da mensagem)', teclas: ['/'] },
        { grupo: 'Chat', rotulo: 'Buscar na conversa', teclas: ['Ctrl', 'F'], quando: e => ctrl(e) && !e.shiftKey && e.key.toLowerCase() === 'f' && atual === 'chat', fn: () => O.busca.abrir() },
        { grupo: 'Chat', rotulo: 'Copiar a última resposta', teclas: ['Ctrl', '⇧', 'C'], quando: e => ctrl(e) && e.shiftKey && e.key.toLowerCase() === 'c', fn: () => O.acoes.copiarUltima() },
        { grupo: 'Chat', rotulo: 'Citar o trecho selecionado', teclas: ['Ctrl', '⇧', 'Q'], quando: e => ctrl(e) && e.shiftKey && e.key.toLowerCase() === 'q', fn: () => { if (!O.chat.citarSelecao?.()) ui.toast('Selecione um trecho de uma mensagem para citar.', { tipo: 'aviso', ms: 2400 }); } },
        { grupo: 'Chat', rotulo: 'Enviar', teclas: ['Enter'] },
        { grupo: 'Chat', rotulo: 'Quebrar a linha', teclas: ['⇧', 'Enter'] },
        { grupo: 'Chat', rotulo: 'Repetir a mensagem anterior', teclas: ['↑'] },
        { grupo: 'Chat', rotulo: 'Parar a resposta', teclas: ['Esc'] },
    ];
    O.atalhos = {
        lista: ATALHOS,
        porGrupo() { return ATALHOS.reduce((m, a) => { (m[a.grupo] = m[a.grupo] || []).push(a); return m; }, {}); },
    };

    function aoTecla(e) {
        if (e.defaultPrevented || e.isComposing) return;
        const alvo = e.target;
        const digitando = ['INPUT', 'TEXTAREA', 'SELECT'].includes(alvo.tagName) || alvo.isContentEditable;
        for (const a of ATALHOS) {
            if (!a.fn || !a.quando(e)) continue;
            if (a.digitando === false && digitando) continue;
            e.preventDefault();
            a.fn();
            return;
        }
        if (e.key !== 'Escape') return;
        if (O.palette.aberta()) { O.palette.fechar(); return; }
        if (O.busca.fechar()) return;
        if (atual === 'chat' && O.chat.ocupado()) { O.chat.parar(); return; }
        if (O.views[atual]?.escape?.()) return;
        if (html.dataset.foco === 'true') { O.acoes.foco(); return; }
        if (atual !== 'home' && atual !== 'chat' && !digitando) ir('home');
        else if (digitando) alvo.blur();
    }

    /* ── janela (só no app desktop) ────────────────────────────────────── */
    function ligarJanela() {
        const ponte = () => window.pywebview?.api;
        $('#btn-min').addEventListener('click', () => ponte()?.minimize_app?.());
        $('#btn-max').addEventListener('click', () => ponte()?.toggle_maximize?.());
        $('#btn-close').addEventListener('click', () => {
            $('#close-overlay').dataset.show = 'true';
            setTimeout(() => { (ponte()?.close_app?.() ?? window.close()); }, 420);
        });
        const mudo = $('#btn-mute'), uso = $('#mute-use');
        const pinta = () => {
            const m = prefs.get('tts_mudo');
            mudo.setAttribute('aria-pressed', String(!m));
            mudo.dataset.tip = m ? 'Resposta por voz desligada' : 'Resposta por voz ligada';
            uso.setAttribute('href', m ? '#i-speaker-off' : '#i-speaker');
        };
        pinta();
        prefs.assinar('tts_mudo', pinta);
        mudo.addEventListener('click', () => O.acoes.alternarTts());
        // o nome do usuário pode vir do app no futuro; por ora a saudação é fixa
        document.addEventListener('contextmenu', e => { if (!e.target.closest('input, textarea, .selectable')) e.preventDefault(); });
    }

    function ligarPonteDesktop() {
        const pronto = async () => {
            html.classList.add('shell-desktop');
            try { const c = await window.pywebview.api.get_config?.(); if (c?.token) api.configurarToken(c.token); } catch (_) { /* app antigo sem get_config */ }
            O.transport.conectarHub();
        };
        if (window.pywebview?.api) pronto();
        else window.addEventListener('pywebviewready', pronto, { once: true });
        if (location.protocol === 'file:') O.transport.conectarHub();
    }

    /* ── atenção: título da aba quando algo precisa de você ───────────── */
    function ligarAtencao() {
        const base = () => (atual === 'home' ? 'Orion' : `${VIEWS[atual].titulo} · Orion`);
        bus.on('atencao', ({ tipo }) => {
            if (document.hasFocus() && !document.hidden) return;
            document.title = `${tipo === 'aprovacao' ? '⚠ Aprovação pendente' : '● Nova resposta'} · Orion`;
        });
        window.addEventListener('focus', () => { document.title = base(); });
    }

    /* ── boot ──────────────────────────────────────────────────────────── */
    const esperar = ms => new Promise(r => setTimeout(r, ms));
    async function boot() {
        const tela = $('#boot'), linhas = $('#boot-lines');
        let visto = false;
        try { visto = sessionStorage.getItem('orion_boot') === '1'; } catch (_) { /* sem sessionStorage */ }
        if (O.movimentoReduzido() || visto || new URLSearchParams(location.search).has('semboot')) {
            tela.remove();
            O.sky.revelar();
            return;
        }
        $('#app').inert = true;
        const linha = texto => {
            const d = document.createElement('div');
            d.className = 'boot-line';
            d.textContent = texto;
            linhas.append(d);
            requestAnimationFrame(() => { d.dataset.show = 'true'; });
            return d;
        };
        tela.dataset.in = 'true';
        await esperar(480);
        const l1 = linha('cérebro …');
        const r = await api.ping(1500);
        l1.textContent = `cérebro · ${r.ok ? `ok (${r.ms} ms)` : 'offline'}`;
        if (!r.ok) l1.dataset.bad = 'true';
        await esperar(320);
        linha('pronto');
        await esperar(380);
        $('#app').inert = false;
        O.sky.revelar();
        O.som.boot();
        tela.dataset.hide = 'true';
        setTimeout(() => tela.remove(), 700);
        try { sessionStorage.setItem('orion_boot', '1'); } catch (_) { /* tudo bem repetir o boot */ }
    }

    /* ── início ────────────────────────────────────────────────────────── */
    async function init() {
        O.aplicarPrefs();
        await api.detectar();
        O.sky.init($('#sky'));
        O.chat.init();
        O.composer.init();
        O.sidebar.init();
        O.palette.init();
        O.busca.init();
        for (const v of Object.values(O.views)) v.init?.();
        O.voz.ligar();
        const capacidades = () => {
            const controles = { model_selection: '#model-btn, #cfg-model', upload: '#btn-attach',
                voice: '.voice-live-btn', tts: '#cfg-tts, #btn-mute, [data-acao=ouvir]', sessions: '#sb-new' };
            for (const [recurso, seletor] of Object.entries(controles)) {
                document.querySelectorAll(seletor).forEach(b => {
                    b.disabled = !api.suporta(recurso);
                    b.title = b.disabled ? 'Ainda indisponível neste backend' : '';
                });
            }
            let aviso = $('#chat-capabilities');
            const c = api.estado();
            if (c.backend === 'orion' && !api.suporta('chat')) {
                const texto = c.unavailable?.chat === 'auth_not_configured' ? 'O acesso ao chat ainda não foi configurado no servidor.' : 'O modelo ainda está indisponível. Você pode continuar escrevendo seu rascunho.';
                if (!aviso) { aviso = el('p', { id: 'chat-capabilities', class: 'banner banner-warn', role: 'status' }); $('#composer .composer-inner').prepend(aviso); }
                aviso.textContent = texto;
            } else aviso?.remove();
            O.composer.atualizar();
        };
        bus.on('capabilities', capacidades);
        capacidades();
        ligarJanela();
        ligarAtencao();
        document.addEventListener('keydown', aoTecla);
        window.addEventListener('hashchange', () => { const o = opcoesPendentes; opcoesPendentes = {}; mostrar(viewDaUrl(), o, false); });
        bus.on('sessoes', subtituloDoChat);
        mostrar(viewDaUrl(), {}, true);
        ligarPonteDesktop();
        api.ttsMudo(!!prefs.get('tts_mudo')).catch(() => { /* cérebro fora: sincroniza na próxima */ });
        boot().then(() => O.chat.carregarPendentes());
        // compatibilidade: o app desktop chama estas funções por evaluate_js
        window.setOrionState = s => O.estado.definir(s);
        window.setAudioIntensity = v => bus.emit('audio', v);
        document.documentElement.dataset.pronto = 'true';
    }

    O.app = { ir, view: () => atual };
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
    else init();
})();
