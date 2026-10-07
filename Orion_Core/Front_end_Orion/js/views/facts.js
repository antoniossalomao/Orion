/* Memória legível: lista de fatos com fonte, data e aprovação explícita. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus, ui } = O, graph = O.views.memoria;
    let active = false, showingGraph = false, generation = 0, busy = false, root, status, list, pending, search;
    const origin = () => `${api.base()}:${api.estado().backend}`, scope = () => O.projects.current()?.id || null;
    const button = (text, fn, cls = 'btn btn-outline btn-sm') => el('button', { type: 'button', class: cls, text, on: { click: fn } });
    const hint = text => el('p', { class: 'extension-hint', text });
    const field = (label, input) => el('label', { class: 'extension-field' }, el('span', { text: label }), input);
    async function operation(fn) {
        if (busy) return; busy = true; const source = origin(), context = scope();
        try { await fn(); if (source === origin() && context === scope()) await load(); }
        catch (error) { if (source === origin()) ui.toast(error.message, { tipo: 'erro', id: 'facts-operation' }); }
        finally { busy = false; }
    }
    async function review(row, action) {
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para revisar a memória.', { tipo: 'aviso' }); return; }
        const source = origin(), context = scope();
        const text = el('textarea', { class: 'input', rows: '4', maxlength: '16000', 'aria-label': 'Texto do fato', text: row.text });
        const accepted = await O.extensions.dialog(action === 'edit' ? 'Corrigir fato' : 'Esquecer fato',
            action === 'edit' ? 'Revise o texto. A fonte original será preservada. A alteração só acontece após sua aprovação.' : 'O texto, o índice de busca e o vetor serão removidos. Você ainda precisa aprovar esta ação.',
            [hint(`Fonte: ${row.source}`), ...(action === 'edit' ? [field('Texto do fato', text)] : [hint(row.text)])], 'Pedir aprovação');
        if (!accepted || source !== origin() || context !== scope()) return;
        await operation(() => api.revisarFato(row.id, context, { action, ...(action === 'edit' ? { text: text.value } : {}), session_id: O.historico.sessao() }));
    }
    function approvalCard(row) {
        const context = scope(), source = origin();
        const approve = button(row.status === 'approved' ? 'Executar alteração aprovada' : 'Aprovar e executar', () => operation(async () => {
            if (row.status !== 'approved') await api.decidir(row.id, true);
            if (source !== origin() || context !== scope()) return;
            await api.executarFato(row.fact_id, context, row.id); bus.emit('sessoes:atualizar');
        }), 'btn btn-primary btn-sm');
        return el('article', { class: 'card fact-approval', dataset: { factApproval: row.id } }, el('h3', { text: row.tool === 'editar_fato' ? 'Revisar correção' : 'Confirmar esquecimento' }),
            row.text ? el('p', { class: 'fact-text selectable', text: row.text }) : null,
            row.tool === 'esquecer_fato' ? hint('Remoção definitiva após aprovação.') : null,
            el('div', { class: 'extension-actions' }, approve, row.status !== 'approved' ? button('Rejeitar', () => operation(() => api.decidir(row.id, false))) : null));
    }
    async function load() {
        const token = ++generation, context = scope(), source = origin();
        if (!api.suporta('memory_facts')) return;
        status.textContent = 'Carregando fatos…';
        try {
            const [rows, approvals] = await Promise.all([api.fatos(context, search.value), api.aprovacoesFatos(context)]);
            if (token !== generation || context !== scope() || source !== origin()) return;
            pending.replaceChildren(...approvals.map(approvalCard));
            list.replaceChildren(...rows.map(row => el('article', { class: 'card fact-item', dataset: { factId: String(row.id) } },
                el('p', { class: 'fact-text selectable', text: row.text }),
                hint(`Fonte: ${row.source} · atualizado em ${new Date(row.updated_at * 1000).toLocaleDateString('pt-BR')}`),
                el('div', { class: 'extension-actions' }, button('Corrigir', () => review(row, 'edit')), button('Esquecer', () => review(row, 'forget'), 'btn btn-ghost btn-sm')))));
            status.textContent = `${context ? 'Memória de ' + O.projects.current().name : 'Memória pessoal'} · ${rows.length ? rows.length + ' fato(s)' : 'Nenhum fato neste contexto.'}`;
        } catch (error) { if (token === generation) status.textContent = `Não consegui carregar: ${error.message}`; }
    }
    function display() {
        const facts = api.suporta('memory_facts') && !showingGraph;
        root.hidden = !facts; $('.mem-toolbar').hidden = facts; $('.mem-body').hidden = facts;
        if (facts) { graph.desativar(); load(); } else graph.ativar();
    }
    O.views.memoria = {
        init() {
            graph.init();
            search = el('input', { class: 'input', type: 'search', 'aria-label': 'Buscar fatos', placeholder: 'Buscar fatos com fonte…' });
            status = el('p', { id: 'facts-status', role: 'status' }); list = el('div', { id: 'facts-list', class: 'fact-list' }); pending = el('section', { id: 'fact-approvals', 'aria-label': 'Revisões da memória' });
            const graphButton = button('Ver grafo', () => { showingGraph = true; display(); });
            root = el('div', { id: 'facts-root', class: 'facts-root' }, el('div', { class: 'page-inner' },
                el('div', { class: 'page-head' }, el('div', {}, el('h2', { text: 'Sua memória' }), hint('Fatos que você pode conferir, corrigir e esquecer.')), graphButton),
                field('Buscar fatos', search), status, pending, list));
            $('#view-memoria').append(root);
            const back = button('Ver fatos', () => { showingGraph = false; display(); }); $('.mem-toolbar').prepend(back);
            const controls = () => { graphButton.disabled = !api.suporta('memory_graph'); back.hidden = !api.suporta('memory_facts'); };
            controls(); search.addEventListener('input', O.util.debounce(load, 200));
            bus.on('sessoes', () => { ++generation; if (active && !showingGraph) { list.replaceChildren(); pending.replaceChildren(); load(); } });
            bus.on('capabilities', () => { ++generation; list.replaceChildren(); pending.replaceChildren(); showingGraph = false; controls(); if (active) display(); });
        },
        ativar() { active = true; display(); }, desativar() { active = false; ++generation; graph.desativar(); },
        escape() { return showingGraph ? graph.escape() : false; }, ativoAgora: () => active
    };
})();
