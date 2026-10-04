/* ==========================================================================
   ORION — sidebar.js | conversas, busca, recolher, estado da conexão
   Barra fixa, recolhível a só ícones (Ctrl+B); em janela estreita (≤ 860 px) fica sempre recolhida.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, bus, ui, api, prefs } = O;
    const U = O.util;

    let sessoes = [], ativa = null, filtro = '', online = null, listaOnline = null, abrindo = false;
    let lista, busca;

    /* ── lista de conversas ────────────────────────────────────────────── */
    function titulo(s) { return s.titulo || 'Sem título'; }

    function comMarcas(texto, consulta) {
        if (!consulta) return [texto];
        const pos = new Set(O.fuzzy.contem(consulta, texto).marcas);
        if (!pos.size) return [texto];
        const saida = [];
        let acc = '', marcado = false;
        for (let i = 0; i < texto.length; i++) {
            const m = pos.has(i);
            if (m !== marcado && acc) { saida.push(marcado ? el('mark', { text: acc }) : acc); acc = ''; }
            marcado = m; acc += texto[i];
        }
        if (acc) saida.push(marcado ? el('mark', { text: acc }) : acc);
        return saida;
    }

    function botaoConversa(s) {
        const atual = s.sessao_id === ativa;
        const b = el('button', { class: 'conv', type: 'button', 'aria-current': atual ? 'true' : null, title: titulo(s), dataset: { id: s.sessao_id } },
            el('span', { class: 'conv-title' }, ...comMarcas(titulo(s), filtro)),
            s.somente_leitura ? el('span', { class: 'conv-ro', text: 'leitura' }) : null);
        b.addEventListener('click', () => abrir(s));
        return b;
    }

    function desenhar() {
        lista.setAttribute('aria-busy', 'false');
        if (!sessoes.length) {
            lista.replaceChildren(el('div', { class: 'sb-empty', text: !api.suporta('sessions') && api.estado().api === 'online' ? 'Conversas salvas ainda indisponíveis neste backend.' : listaOnline === false
                ? 'Não foi possível carregar as conversas. Tentando novamente…' : 'Nenhuma conversa ainda. Comece uma nova.' }));
            return;
        }
        if (filtro) {
            const achadas = O.fuzzy.buscar(sessoes, filtro, titulo);
            lista.replaceChildren(...(achadas.length ? achadas.map(botaoConversa)
                : [el('div', { class: 'sb-empty', text: `Nada encontrado para “${filtro}”.` })]));
            return;
        }
        const nos = [];
        for (const g of U.agruparPorDia(sessoes, s => s.criada)) {
            nos.push(el('div', { class: 'conv-group', text: g.rotulo }), ...g.itens.map(botaoConversa));
        }
        lista.replaceChildren(...nos);
    }

    async function carregar() {
        try {
            const d = await api.sessoes();
            sessoes = d.sessoes || [];
            ativa = d.ativa || sessoes.find(s => s.ativa)?.sessao_id || null;
            listaOnline = true;
        } catch (e) { listaOnline = false; if (e.indisponivel) { sessoes = []; ativa = null; } }
        desenhar();
        bus.emit('sessoes', { lista: sessoes, ativa });
    }

    async function abrir(s) {
        if (abrindo) return;
        if (s.sessao_id === ativa && !s.somente_leitura) { O.app.ir('chat'); return; }
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para trocar de conversa.', { tipo: 'aviso' }); return; }
        abrindo = true;
        try {
            let msgs;
            if (s.somente_leitura) msgs = (await api.historico(s.sessao_id)).mensagens || [];
            else {
                const d = await api.ativarSessao(s.sessao_id);
                if (d.erro) throw new Error(d.erro);
                msgs = d.mensagens || [];
            }
            O.chat.limpar();
            O.chat.renderHistorico(msgs);
            if (s.somente_leitura) O.chat.nota('Sessão antiga, somente leitura.');
            O.app.ir('chat');
            await carregar();
        } catch (e) {
            ui.toast(`Não consegui abrir a conversa: ${e.message}`, { tipo: 'erro' });
        } finally { abrindo = false; }
    }

    async function nova() {
        if (!api.suporta('sessions')) { ui.toast('Conversas salvas ainda indisponíveis neste backend.', { tipo: 'aviso' }); return; }
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para começar outra conversa.', { tipo: 'aviso' }); return; }
        try {
            const d = await api.novaSessao();
            if (d && d.erro) throw new Error(d.erro);
            O.chat.limpar();
            await carregar();
        } catch (e) {
            ui.toast(`Cérebro fora do ar: não deu para criar a conversa. ${e.rede ? '' : e.message}`.trim(), { tipo: 'erro' });
            O.chat.limpar();
        }
        O.app.ir('chat');
        O.composer.foco();
    }

    /* ── recolher ──────────────────────────────────────────────────────── */
    function atualizarToggle() {
        const rec = prefs.get('sb') === 'collapsed';
        const t = $('#sb-toggle');
        t.setAttribute('aria-expanded', String(!rec));
        t.setAttribute('aria-label', rec ? 'Expandir barra lateral' : 'Recolher barra lateral');
        t.dataset.tip = rec ? 'Expandir (Ctrl+B)' : 'Recolher (Ctrl+B)';
    }
    function alternar() {
        if (O.estreita()) return;      // em janela estreita a barra fica sempre recolhida
        prefs.set('sb', prefs.get('sb') === 'collapsed' ? 'expanded' : 'collapsed');
        atualizarToggle();
    }

    /* ── conexão ───────────────────────────────────────────────────────── */
    let verificando = false;
    async function verificar() {
        if (document.hidden || verificando) return;
        verificando = true;
        try {
            const r = await api.ping();
            const antes = online;
            online = r.ok;
            $('#conn-dot').dataset.state = r.ok ? (r.model === 'ready' ? 'ok' : 'warn') : 'danger';
            const texto = r.ok ? (r.incompatible ? 'Versão incompatível' : r.model === 'unavailable' ? 'Modelo indisponível' : r.model === 'unknown' ? 'API disponível' : 'Conectado') : 'Sem conexão';
            // O status só é anunciado quando muda, nunca a cada polling.
            if ($('#conn-text').textContent !== texto) {
                $('#conn-text').textContent = texto;
                // A conexão continua identificável quando só o trilho de ícones está visível.
                $('#conn').setAttribute('aria-label', texto);
                $('#conn').title = texto;
            }
            const sub = r.ok ? (r.model === 'unavailable' ? 'API disponível' : '') : 'tentando de novo…';
            if ($('#conn-sub').textContent !== sub) $('#conn-sub').textContent = sub;
            if (antes !== online) {
                bus.emit('conn', { ok: r.ok, ms: r.ms });
                if (r.ok && antes === false) carregar();
            }
        } finally { verificando = false; }
    }

    function init() {
        lista = $('#sb-convs-list'); busca = $('#sb-search');
        $('#sb-toggle').addEventListener('click', alternar);
        $('#sb-new').addEventListener('click', nova);
        busca.addEventListener('input', () => { filtro = busca.value.trim(); desenhar(); });
        busca.addEventListener('keydown', e => { if (e.key === 'Escape' && busca.value) { e.stopPropagation(); busca.value = ''; filtro = ''; desenhar(); } });
        lista.addEventListener('keydown', e => {
            if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
            const itens = $$('.conv', lista), i = itens.indexOf(document.activeElement);
            if (i < 0) return;
            e.preventDefault();
            itens[Math.max(0, Math.min(itens.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))].focus();
        });
        atualizarToggle();
        const atualizarLista = U.debounce(carregar, 600);
        bus.on('sessoes:atualizar', atualizarLista);
        carregar();
        verificar();
        setInterval(verificar, 8000);
        setInterval(() => { if (!document.hidden) carregar(); }, 20000);   // pega o título automático da 1ª mensagem
        document.addEventListener('visibilitychange', () => { if (!document.hidden) { verificar(); carregar(); } });
    }

    O.sidebar = {
        init, nova, alternar, carregar, abrir, verificar,
        sessoes: () => sessoes, ativa: () => sessoes.find(s => s.sessao_id === ativa) || null,
        online: () => online,
    };
})();
