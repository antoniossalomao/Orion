/* Clientes externos: segredo só na criação; acesso de leitura por contexto. */
(function () {
    'use strict';
    const O = window.Orion, { el, api, bus } = O, X = O.extensions;
    let root, active = false, generation = 0;
    const permissions = { search: 'Buscar contexto', facts: 'Ler fatos', sources: 'Ler fontes indexadas', artifacts: 'Ler resultados' };
    async function issue() {
        const source = api.base(), name = el('input', { class: 'input', 'aria-label': 'Nome do cliente' }), context = O.projects.options('Contexto do cliente'), hours = el('input', { class: 'input', type: 'number', min: '1', max: '720', value: '168', 'aria-label': 'Validade em horas' });
        const checks = Object.entries(permissions).map(([key, label]) => ({ key, ...X.check(label) }));
        const field = (label, input) => el('label', { class: 'extension-field' }, el('span', { text: label }), input);
        if (!await X.dialog('Autorizar cliente externo', 'Conceda somente as leituras necessárias. Este token não permite administrar o Orion.', [field('Nome do cliente', name), field('Contexto do cliente', context), field('Validade em horas', hours), ...checks.map(c => c.row)], 'Gerar credencial') || source !== api.base()) return;
        try {
            const row = await api.criarClienteExterno({ name: name.value, project_id: context.value === 'personal' ? null : context.value.replace(/^project:/, ''), lifetime_hours: Number(hours.value), permissions: checks.filter(c => c.input.checked).map(c => c.key) });
            if (source !== api.base()) return;
            const token = el('textarea', { class: 'input', readonly: '', rows: '3', 'aria-label': 'Credencial do cliente' });
            token.value = row.token;
            await X.dialog('Credencial criada', 'Copie agora para o cliente escolhido. Ela será exibida apenas uma vez. Você pode revogá-la nesta tela.', [token, X.hint(`Endpoint: ${api.base()}/mcp-export/rpc`), X.hint('Use Authorization: Bearer com esta credencial. Não use o token administrativo.')], 'Guardei a credencial');
            token.value = ''; await load();
        } catch (error) { O.ui.toast(`Não foi possível criar a credencial: ${error.message}`, { tipo: 'aviso' }); }
    }
    async function revoke(row) {
        const source = api.base();
        if (!await X.dialog('Revogar cliente', `${row.name} deixará de ler novos dados do Orion.`, [], 'Revogar acesso') || source !== api.base()) return;
        try { await api.revogarClienteExterno(row.id); if (source === api.base()) await load(); }
        catch (error) { O.ui.toast(error.message, { tipo: 'aviso' }); }
    }
    async function load() {
        const token = ++generation, source = api.base(); root.hidden = !api.suporta('mcp_export');
        if (root.hidden) return;
        try {
            const rows = await api.clientesExternos();
            if (token !== generation || source !== api.base() || !active) return;
            root.replaceChildren(el('h4', { text: 'Clientes externos' }), X.hint('Permita que outro app consulte um contexto do Orion por MCP, com acesso de leitura e prazo definido.'), X.button('Autorizar cliente externo', issue), ...rows.map(row => el('article', { class: 'fact-item', dataset: { exportClient: row.id } }, el('h5', { text: row.name }), X.hint(row.project_id ? (O.projects.options('Contexto', 'project:' + row.project_id).selectedOptions[0]?.textContent || 'Projeto') : 'Pessoal'), X.hint(row.permissions.map(p => permissions[p]).join(' · ')), X.hint(row.revoked ? 'Revogado' : row.expires_at * 1000 <= Date.now() ? 'Expirado' : `Expira em ${new Date(row.expires_at * 1000).toLocaleString('pt-BR')}`), row.revoked ? null : X.button('Revogar acesso', () => revoke(row)))));
        } catch (error) { if (token === generation) root.replaceChildren(X.hint(`Não foi possível carregar clientes: ${error.message}`)); }
    }
    O.exportClients = {
        init(parent) { root = el('div', { class: 'calendar-section' }); root.hidden = true; parent.append(root); bus.on('capabilities', () => { generation++; if (active) load(); }); },
        activate() { active = true; load(); }, deactivate() { active = false; generation++; }
    };
})();
