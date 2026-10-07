/* ==========================================================================
   ORION — sidebar.js | conversas, busca, recolher, estado da conexão
   Barra fixa, recolhível a só ícones (Ctrl+B); em janela estreita (≤ 860 px) fica sempre recolhida.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, icone, bus, ui, api, prefs } = O;
    const U = O.util;

    let sessoes = [], ativa = null, filtro = '', online = null, abrindo = false;
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
            s.favorita ? el('span', { class: 'conv-pin', html: icone('pin'), 'aria-label': 'Fixada', role: 'img' }) : null,
            el('span', { class: 'conv-title' }, ...comMarcas(titulo(s), filtro)),
            s.somente_leitura ? el('span', { class: 'conv-ro', text: 'leitura' }) : null);
        b.addEventListener('click', () => abrir(s));
        // conversa importada (somente leitura) não tem ações; as demais têm o menu ⋯ (Renomear, Fixar, Apagar)
        const mais = s.somente_leitura ? null : el('button', {
            class: 'icon-btn conv-more', type: 'button', 'aria-label': `Ações da conversa ${titulo(s)}`,
            'aria-haspopup': 'menu', 'aria-expanded': 'false', html: icone('more'),
        });
        mais?.addEventListener('click', e => { e.stopPropagation(); alternarMenu(s, mais); });
        return el('div', { class: 'conv-row', dataset: { id: s.sessao_id } }, b, mais);
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
        const fixadas = sessoes.filter(s => s.favorita);
        if (fixadas.length) nos.push(el('div', { class: 'conv-group', text: 'Fixadas' }), ...fixadas.map(botaoConversa));
        for (const g of U.agruparPorDia(sessoes.filter(s => !s.favorita), s => s.ultima_atividade || s.criada)) {
            nos.push(el('div', { class: 'conv-group', text: g.rotulo }), ...g.itens.map(botaoConversa));
        }
        lista.replaceChildren(...nos);
    }

    /* ── ações da conversa: renomear, fixar, apagar (menu ⋯, paleta) ─────── */
    let menu = null, menuDono = null, menuBotao = null;
    const rotuloFixar = s => (s.favorita ? 'Desafixar' : 'Fixar no topo');

    function fecharMenu(devolverFoco = true) {
        if (!menu || menu.dataset.open !== 'true') return;
        menu.dataset.open = 'false';
        menuBotao?.setAttribute('aria-expanded', 'false');
        if (devolverFoco) menuBotao?.focus();
        menuDono = null;
    }

    function montarMenu() {
        menu = el('div', { id: 'conv-menu', class: 'menu conv-menu', role: 'menu', 'aria-label': 'Ações da conversa', dataset: { open: 'false' } });
        $('#main').append(menu);   // dentro do <main>: fora de um marco o axe reclama (regra "region"); fixed não é cortado aqui
        menu.addEventListener('keydown', e => {
            const itens = $$('[role="menuitem"]', menu), i = itens.indexOf(document.activeElement);
            if (e.key === 'ArrowDown') { e.preventDefault(); itens[(i + 1) % itens.length].focus(); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); itens[(i - 1 + itens.length) % itens.length].focus(); }
            else if (e.key === 'Home') { e.preventDefault(); itens[0].focus(); }
            else if (e.key === 'End') { e.preventDefault(); itens[itens.length - 1].focus(); }
            else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); fecharMenu(); }
            else if (e.key === 'Tab') fecharMenu(false);
        });
        document.addEventListener('pointerdown', e => {
            if (menu.dataset.open === 'true' && !menu.contains(e.target) && !e.target.closest?.('.conv-more')) fecharMenu(false);
        });
    }

    function alternarMenu(s, botao) {
        if (!menu) montarMenu();
        if (menu.dataset.open === 'true' && menuDono?.sessao_id === s.sessao_id) { fecharMenu(); return; }
        fecharMenu(false);
        menuDono = s; menuBotao = botao;
        const item = (acao, rotulo, perigo = false) => el('button', {
            class: `menu-item${perigo ? ' menu-item-perigo' : ''}`, type: 'button', role: 'menuitem', text: rotulo,
            on: { click: () => { const dono = menuDono; fecharMenu(false); if (acao === 'renomear') renomear(dono); else if (acao === 'fixar') alternarFixa(dono); else apagar(dono); } },
        });
        menu.replaceChildren(item('renomear', 'Renomear'), item('fixar', rotuloFixar(s)), item('apagar', 'Apagar', true));
        const r = botao.getBoundingClientRect();
        menu.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - 190))}px`;
        menu.style.top = `${Math.min(r.bottom + 4, window.innerHeight - 140)}px`;
        menu.dataset.open = 'true';
        botao.setAttribute('aria-expanded', 'true');
        menu.querySelector('[role="menuitem"]')?.focus();
    }

    async function executar(fn, falha) {
        try { await fn(); await carregar(); return true; }
        catch (e) { ui.toast(`${falha} ${e.message}`.trim(), { tipo: 'erro' }); return false; }
    }

    async function renomear(s) {
        if (!s || s.somente_leitura) return;
        const novo = await ui.perguntar({ titulo: 'Renomear conversa', rotulo: 'Título', valor: titulo(s), ok: 'Salvar' });
        if (!novo || novo === titulo(s)) return;
        if (await executar(() => api.renomearSessao(s.sessao_id, novo), 'Não consegui renomear.')) O.anunciar(`Conversa renomeada para ${novo}.`);
    }

    async function alternarFixa(s) {
        if (!s || s.somente_leitura) return;
        const fixar = !s.favorita;
        if (await executar(() => api.fixarSessao(s.sessao_id, fixar), fixar ? 'Não consegui fixar.' : 'Não consegui desafixar.')) {
            O.anunciar(fixar ? 'Conversa fixada no topo.' : 'Conversa desafixada.');
        }
    }

    async function apagar(s) {
        if (!s || s.somente_leitura) return;
        const eraAtiva = s.sessao_id === ativa;
        if (eraAtiva && O.chat.ocupado()) { ui.toast('Espere a resposta terminar para apagar a conversa.', { tipo: 'aviso' }); return; }
        const ok = await ui.confirmar({
            titulo: 'Apagar esta conversa?',
            texto: `“${titulo(s)}” sai da lista. As mensagens ficam guardadas no banco e o que o Orion já aprendeu delas continua na memória.`,
            ok: 'Apagar', perigo: true,
        });
        if (!ok) return;
        if (!await executar(() => api.apagarSessao(s.sessao_id), 'Não consegui apagar.')) return;
        O.anunciar('Conversa apagada.');
        if (eraAtiva) {
            // o cérebro escolheu outra conversa como ativa: mostra o histórico dela (ou a tela vazia)
            O.chat.limpar();
            const nova = sessoes.find(x => x.sessao_id === ativa);
            if (nova && !nova.somente_leitura) {
                try { O.chat.renderHistorico((await api.ativarSessao(nova.sessao_id)).mensagens || []); } catch (_) { /* fica vazia */ }
            }
        }
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
        init, nova, alternar, carregar, abrir, renomear, alternarFixa, apagar,
        sessoes: () => sessoes, ativa: () => sessoes.find(s => s.sessao_id === ativa) || null,
        online: () => online,
    };
})();
