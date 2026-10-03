/* ==========================================================================
   ORION — sidebar.js | conversas, busca, recolher/gaveta, estado da conexão
   Desktop: barra fixa (recolhível a só ícones). ≤ 860 px: gaveta com foco preso e
   `inert` quando fechada — nada escondido por transform fica alcançável por Tab.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, bus, ui, api, prefs } = O;
    const U = O.util;

    let sessoes = [], ativa = null, filtro = '', online = null, abrindo = false;
    let btnMenu, sidebar, lista, busca;
    const html = document.documentElement;

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
            lista.replaceChildren(el('div', { class: 'sb-empty', text: online === false
                ? 'Cérebro offline. As conversas aparecem quando ele voltar.' : 'Nenhuma conversa ainda. Comece uma nova.' }));
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
            online = true;
        } catch (_) { online = false; }
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

    /* ── recolher e gaveta ─────────────────────────────────────────────── */
    function atualizarToggle() {
        const rec = prefs.get('sb') === 'collapsed';
        const t = $('#sb-toggle');
        t.setAttribute('aria-expanded', String(!rec));
        t.setAttribute('aria-label', rec ? 'Expandir barra lateral' : 'Recolher barra lateral');
        t.dataset.tip = rec ? 'Expandir (Ctrl+B)' : 'Recolher (Ctrl+B)';
    }
    function alternar() {
        if (O.celular()) { gavetaAberta() ? fecharGaveta() : abrirGaveta(); return; }
        prefs.set('sb', prefs.get('sb') === 'collapsed' ? 'expanded' : 'collapsed');
        atualizarToggle();
    }

    const gavetaAberta = () => html.dataset.drawer === 'open';
    function ajustarInert() {
        sidebar.inert = O.celular() && !gavetaAberta();
        btnMenu.setAttribute('aria-expanded', String(gavetaAberta()));
    }
    let soltarFoco = null;
    function abrirGaveta() {
        html.dataset.drawer = 'open';
        ajustarInert();
        soltarFoco = ui.prenderFoco(sidebar, fecharGaveta);
        setTimeout(() => $('#sb-new').focus(), 60);
    }
    function fecharGaveta(devolverFoco = true) {
        if (!gavetaAberta()) return false;
        delete html.dataset.drawer;
        soltarFoco?.(); soltarFoco = null;
        ajustarInert();
        if (devolverFoco) btnMenu.focus();
        return true;
    }

    /* ── conexão ───────────────────────────────────────────────────────── */
    let primeiraVez = true;
    async function verificar() {
        if (document.hidden) return;
        const r = await api.ping();
        const antes = online;
        online = r.ok;
        const dot = $('#conn-dot');
        dot.dataset.state = r.ok ? 'ok' : 'danger';
        $('#conn-text').textContent = r.ok ? 'Conectado' : 'Sem conexão';
        $('#conn-sub').textContent = r.ok ? `${r.ms} ms` : 'tentando de novo…';
        if (antes !== online) {
            bus.emit('conn', { ok: r.ok, ms: r.ms });
            if (!primeiraVez) {
                if (!r.ok) ui.toast('O cérebro parou de responder. Vou continuar tentando.', { tipo: 'aviso', ms: 0, id: 'conn' });
                else { ui.toast('Cérebro de volta.', { tipo: 'ok', id: 'conn' }); carregar(); }
            }
        }
        primeiraVez = false;
    }

    function init() {
        btnMenu = $('#menu-btn'); sidebar = $('#sidebar'); lista = $('#sb-convs-list'); busca = $('#sb-search');
        $('#sb-toggle').addEventListener('click', alternar);
        btnMenu.addEventListener('click', () => (gavetaAberta() ? fecharGaveta() : abrirGaveta()));
        $('#drawer-scrim').addEventListener('click', () => fecharGaveta());
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
        window.matchMedia('(max-width: 860px)').addEventListener('change', () => { if (!O.celular()) fecharGaveta(false); ajustarInert(); });
        atualizarToggle();
        ajustarInert();
        const atualizarLista = U.debounce(carregar, 600);
        bus.on('sessoes:atualizar', atualizarLista);
        carregar();
        verificar();
        setInterval(verificar, 8000);
        setInterval(() => { if (!document.hidden) carregar(); }, 20000);   // pega o título automático da 1ª mensagem
        document.addEventListener('visibilitychange', () => { if (!document.hidden) { verificar(); carregar(); } });
    }

    O.sidebar = {
        init, nova, alternar, abrirGaveta, fecharGaveta, gavetaAberta, carregar, abrir,
        sessoes: () => sessoes, ativa: () => sessoes.find(s => s.sessao_id === ativa) || null,
        online: () => online,
    };
})();
