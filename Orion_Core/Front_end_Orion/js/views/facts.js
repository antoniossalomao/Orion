/* Memória legível: lista de fatos com fonte, data e aprovação explícita. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus, ui } = O, graph = O.views.memoria;
    let active = false, showingGraph = false, generation = 0, busy = false, root, status, list, pending, search, conflicts;
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
    /** "Projeto" do fato: onde ele vale (pessoal ou um projeto). Mover é um clique seu, sem o modelo. */
    function projectField(row, context) {
        const select = O.projects.options(`Projeto do fato: ${row.text.slice(0, 40)}`, context ? `project:${context}` : 'personal');
        select.addEventListener('change', () => {
            const target = select.value === 'personal' ? null : select.value.slice(8);
            select.disabled = true;
            operation(() => api.moverFato(row.id, context, target)).finally(() => { select.disabled = false; });
        });
        return el('label', { class: 'extension-field fact-project' }, el('span', { text: 'Projeto' }), select);
    }
    /** validade: depois da data o fato pesa menos na busca e leva a marca "(antigo)"; nada é apagado */
    async function validity(row, context) {
        const input = el('input', { class: 'input', type: 'date', 'aria-label': 'Vale até', value: row.valid_until ? new Date(row.valid_until * 1000).toISOString().slice(0, 10) : '' });
        const ok = await O.extensions.dialog('Validade do fato', 'Depois desta data o fato pesa menos na busca e aparece como “(antigo)”. Deixe vazio para não ter prazo.',
            [hint(row.text), field('Vale até', input)], 'Salvar');
        if (!ok) return;
        const when = input.value ? Date.parse(`${input.value}T23:59:59`) / 1000 : null;
        await operation(() => api.validadeFato(row.id, context, when));
    }
    function conflictCard(pair, context) {
        const { older, newer } = pair, body = { older_id: older.id, newer_id: newer.id };
        const decide = decision => operation(() => api.resolverConflito(context, { ...body, decision }));
        return el('article', { class: 'card fact-approval', dataset: { conflito: `${older.id}:${newer.id}` } },
            el('h3', { text: 'Estes fatos parecem se contradizer' }),
            el('p', { class: 'fact-text selectable', text: `Antes: ${older.text}` }), el('p', { class: 'fact-text selectable', text: `Agora: ${newer.text}` }),
            hint('O Orion nunca escolhe sozinho qual vale. Nada é apagado.'),
            el('div', { class: 'extension-actions' }, button('O novo vale', () => decide('novo'), 'btn btn-primary btn-sm'), button('Os dois valem', () => decide('ambos'))));
    }
    async function load() {
        const token = ++generation, context = scope(), source = origin();
        if (!api.suporta('memory_facts')) return;
        status.textContent = 'Carregando fatos…';
        try {
            const [rows, approvals, pairs] = await Promise.all([api.fatos(context, search.value), api.aprovacoesFatos(context), api.conflitosFatos(context).catch(() => [])]);
            if (token !== generation || context !== scope() || source !== origin()) return;
            pending.replaceChildren(...approvals.map(approvalCard));
            conflicts.replaceChildren(...pairs.map(p => conflictCard(p, context)));
            list.replaceChildren(...rows.map(row => el('article', { class: 'card fact-item', dataset: { factId: String(row.id), antigo: String(!!row.old) } },
                el('p', { class: 'fact-text selectable', text: (row.old ? '(antigo) ' : '') + row.text }),
                hint(`Fonte: ${row.source} · atualizado em ${new Date(row.updated_at * 1000).toLocaleDateString('pt-BR')}${row.valid_until ? ` · vale até ${new Date(row.valid_until * 1000).toLocaleDateString('pt-BR')}` : ''}${row.superseded_by ? ' · substituído por outro fato' : ''}`),
                O.projects?.options ? projectField(row, context) : null,
                el('div', { class: 'extension-actions' }, button('Corrigir', () => review(row, 'edit')), button('Validade', () => validity(row, context)), button('Esquecer', () => review(row, 'forget'), 'btn btn-ghost btn-sm')))));
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
            status = el('p', { id: 'facts-status', role: 'status' }); list = el('div', { id: 'facts-list', class: 'fact-list' }); pending = el('section', { id: 'fact-approvals', 'aria-label': 'Revisões da memória' }); conflicts = el('section', { id: 'fact-conflicts', 'aria-label': 'Fatos em conflito' });
            const graphButton = button('Ver grafo', () => { showingGraph = true; display(); });
            root = el('div', { id: 'facts-root', class: 'facts-root' }, el('div', { class: 'page-inner' },
                el('div', { class: 'page-head' }, el('div', {}, el('h2', { text: 'Sua memória' }), hint('Fatos que você pode conferir, corrigir e esquecer.')), graphButton),
                field('Buscar fatos', search), status, conflicts, pending, list));
            $('#view-memoria').append(root);
            const back = button('Ver fatos', () => { showingGraph = false; display(); }); $('.mem-toolbar').prepend(back);
            const controls = () => { graphButton.disabled = !api.suporta('memory_graph'); back.hidden = !api.suporta('memory_facts'); };
            controls(); search.addEventListener('input', O.util.debounce(load, 200));
            bus.on('sessoes', () => { ++generation; if (active && !showingGraph) { list.replaceChildren(); pending.replaceChildren(); conflicts.replaceChildren(); load(); } });
            bus.on('capabilities', () => { ++generation; list.replaceChildren(); pending.replaceChildren(); conflicts.replaceChildren(); showingGraph = false; controls(); if (active) display(); });
        },
        ativar() { active = true; display(); }, desativar() { active = false; ++generation; graph.desativar(); },
        escape() { return showingGraph || !api.suporta('memory_facts') ? graph.escape() : false; }, ativoAgora: () => active
    };
    /** pontes para a paleta, o grafo e `/grafo`: abrir um fato na lista e entrar na visão do grafo */
    O.memoriaFatos = {
        abrirFato(texto) {
            O.app.ir('memoria'); showingGraph = false; display();
            search.value = String(texto || '').slice(0, 60); search.dispatchEvent(new Event('input'));
        },
        async mostrarGrafo() { O.app.ir('memoria'); if (!api.suporta('memory_graph')) return false; showingGraph = true; display(); return true; },
    };
})();
