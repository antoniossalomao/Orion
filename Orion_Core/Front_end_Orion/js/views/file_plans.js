/* Planos imutáveis: cópias revisadas, originais preservados. */
(function () {
    'use strict';
    const O = window.Orion, { el, api, bus } = O, X = O.extensions;
    const scope = () => O.projects.current()?.id || null;
    let root, active = false, generation = 0;
    const states = { draft: 'Plano salvo · nenhum arquivo criado.', pending: 'Aguardando confirmação.', applying: 'Execução interrompida ou em andamento. Confira a pasta.', complete: 'Cópias criadas. Originais preservados.', partial: 'Execução parcial. Confira as cópias antes de preparar outro plano.' };
    const details = row => [X.hint(row.root), ...row.copies.map(item => el('p', { class: 'file-plan-path', text: `${item.source} → ${item.target} · ${Math.ceil(item.size / 1024)} KB` })), X.hint(`${row.skipped.length} arquivo(s) ignorado(s). Originais preservados; destinos existentes não são substituídos.`), ...row.skipped.map(item => X.hint(`Ignorado: ${item.name}`))];
    async function mutate(fn, project, source) {
        try { await fn(); if (project === scope() && source === api.base()) await load(); }
        catch (error) { if (project === scope() && source === api.base()) { O.ui.toast(`Não foi possível concluir: ${error.message}`, { tipo: 'aviso' }); await load(); } }
    }
    async function create() {
        const project = scope(), source = api.base(), session = O.historico.sessao();
        if (!session) return O.ui.toast('Abra uma conversa neste contexto antes de preparar o plano.', { tipo: 'aviso' });
        const input = el('input', { class: 'input', 'aria-label': 'Pasta autorizada', placeholder: 'Caminho completo da pasta', value: O.projects.current()?.root || '' });
        if (!await X.dialog('Planejar cópias', 'Agrupe textos, PDFs e imagens em Organizados. Primeiro você revisa o plano; nenhum arquivo será criado agora.', [el('label', { class: 'extension-field' }, el('span', { text: 'Pasta autorizada' }), input)], 'Preparar plano') || project !== scope() || source !== api.base() || session !== O.historico.sessao()) return;
        await mutate(() => api.proporArquivos(project, { root: input.value, session_id: session }), project, source);
    }
    async function review(row) {
        const project = scope(), source = api.base();
        if (!await X.dialog('Revisar cópias', 'Confira todos os destinos antes de pedir aprovação.', details(row), 'Pedir aprovação') || project !== scope() || source !== api.base()) return;
        await mutate(() => api.revisarArquivos(row.id, project, row.digest), project, source);
    }
    async function apply(row, approval) {
        const project = scope(), source = api.base();
        if (!await X.dialog('Criar cópias revisadas', 'Esta confirmação cria somente as cópias abaixo. Os arquivos originais permanecem na pasta.', details(row), 'Confirmar cópias') || project !== scope() || source !== api.base()) return;
        await mutate(async () => { if (approval.status !== 'approved') await api.decidir(approval.id, true); return api.aplicarArquivos(row.id, project, approval.id); }, project, source);
    }
    async function load() {
        const token = ++generation, project = scope(), source = api.base();
        if (!api.suporta('file_plans')) { root.replaceChildren(); return; }
        try {
            const rows = await api.planosArquivos(project);
            if (token !== generation || project !== scope() || source !== api.base() || !active) return;
            root.replaceChildren(el('h3', { text: 'Organizar cópias' }), X.hint('Revise caminhos antes de criar cópias por tipo. Seus originais são preservados.'), X.button('Planejar cópias', create), ...rows.map(row => {
                const approval = row.approvals[0];
                return el('article', { class: 'card fact-item', dataset: { filePlan: row.id } }, el('h4', { text: `${row.copies.length} cópia(s)` }), ...details(row), X.hint(`${states[row.status] || 'Estado indisponível'} ${row.completed} concluída(s).`), row.review_error ? X.hint('O contexto mudou. Prepare um novo plano.') : ['draft', 'pending'].includes(row.status) ? el('div', { class: 'extension-actions' }, approval ? X.button('Criar cópias revisadas', () => apply(row, approval)) : X.button('Revisar cópias', () => review(row)), approval?.status === 'pending' ? X.button('Rejeitar cópias', () => mutate(() => api.decidir(approval.id, false), project, source)) : null) : null);
            }));
        } catch (error) { if (token === generation) root.replaceChildren(X.hint(`Não foi possível carregar os planos: ${error.message}`)); }
    }
    O.filePlans = {
        init(parent) { root = el('section', { class: 'calendar-section', 'aria-label': 'Planos de cópias' }); parent.append(root); for (const event of ['sessoes', 'capabilities']) bus.on(event, () => { generation++; root.replaceChildren(); if (active) load(); }); },
        activate() { active = true; load(); }, deactivate() { active = false; generation++; }
    };
})();
