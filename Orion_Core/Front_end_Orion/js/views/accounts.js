/* Contas e conexões são identidades distintas; só referências chegam ao formulário MCP. */
(function () {
    'use strict';
    const O = window.Orion, { el, api, ui } = O, X = O.extensions;
    let timer = null;
    const field = (name, control) => el('label', { class: 'extension-field' }, el('span', { text: name }), control);
    async function create() {
        const source = api.base(), name = el('input', { class: 'input', maxlength: '160', 'aria-label': 'Nome da conta' }),
            url = el('input', { class: 'input', 'aria-label': 'Servidor MCP da conta', placeholder: 'https://servidor/mcp' }),
            scopes = el('input', { class: 'input', 'aria-label': 'Escopos OAuth solicitados', placeholder: 'Escopos mínimos separados por espaço' }), context = O.projects.options('Contexto da conta');
        if (!await X.dialog('Adicionar conta MCP', 'Confira o servidor e os escopos. A autorização abre no navegador e os tokens ficam no cofre do sistema.', [field('Nome da conta', name), field('Servidor MCP da conta', url), field('Escopos OAuth solicitados', scopes), field('Contexto da conta', context)], 'Adicionar conta') || source !== api.base()) return;
        await X.mutate(() => api.criarConta({ name: name.value, url: url.value, scopes: scopes.value.trim().split(/\s+/), scope: context.value }));
    }
    async function panel() {
        clearTimeout(timer); const source = api.base(), rows = await api.contas();
        const cards = rows.map(row => el('article', { class: 'card extension-card', dataset: { accountId: row.id } },
            el('h3', { text: row.name }), X.hint(`${row.state} · ${row.scope}`), X.hint(row.url),
            X.hint(`Solicitados: ${row.scopes.join(', ')} · concedidos: ${row.granted.join(', ') || 'Aguardando'}`),
            row.expires_at ? X.hint(`Credencial ${row.expires_at * 1000 < Date.now() ? 'expirada' : 'válida até'} ${new Date(row.expires_at * 1000).toLocaleString('pt-BR')} · renovação automática quando permitida pelo servidor.`) : null,
            row.error ? X.hint('Autorização não concluída. Confira o servidor ou autorize novamente.') : null,
            el('div', { class: 'extension-actions' },
                row.authorization_url ? X.button('Abrir autorização', () => api.abrirExterno(row.authorization_url), 'btn btn-primary btn-sm') : X.button('Autorizar conta', () => X.mutate(() => api.autorizarConta(row.id))),
                row.state !== 'revoked' ? X.button('Revogar conta', async () => { if (await X.dialog('Revogar conta', 'O acesso e as decisões vinculadas à conexão serão encerrados. Conversas e resultados continuam salvos.', [], 'Revogar')) await X.mutate(() => api.revogarConta(row.id)); }) : null)));
        if (rows.some(r => r.state === 'authorizing')) timer = setTimeout(() => { if (source === api.base() && document.documentElement.dataset.view === 'integracoes') X.refresh(); }, 2000);
        return el('section', { 'aria-label': 'Contas MCP' }, el('h3', { text: 'Suas contas' }), X.hint('Autorize uma conta e selecione-a ao configurar uma conexão no mesmo contexto.'), X.button('Adicionar conta MCP', create), el('div', { class: 'extension-grid' }, ...cards));
    }
    O.accounts = { panel };
})();
