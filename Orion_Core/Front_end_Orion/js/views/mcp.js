/* Administração MCP: revisão explícita; testar faz apenas handshake e discovery. */
(function () {
    'use strict';
    const O = window.Orion, { el, api } = O, X = O.extensions;
    const drafts = new Map(), results = new Map();
    const origin = () => `${api.base()}:${api.estado().backend}`;
    const field = (label, input) => el('label', { class: 'extension-field' }, el('span', { text: label }), input);
    const input = (label, value = '', placeholder = '') => el('input', { class: 'input', type: 'text', value, placeholder, 'aria-label': label });
    const MESSAGES = { start_failed: 'Confira o endereço ou o comando e teste novamente.', auth_missing: 'Configure a credencial no cofre e informe sua referência.', auth_failed: 'A conta recusou a conexão. Revise a credencial ou reconecte a conta.', transport_error: 'Confira o endereço ou o comando e tente testar novamente.', start_timeout: 'O servidor demorou para iniciar. Confira se está disponível.', protocol_unsupported: 'A versão do servidor ainda não é compatível.', call_timeout: 'A chamada expirou. Confira o resultado antes de repetir uma ação.' };

    async function form(edit = null) {
        const source = origin(), key = `${source}:${edit || 'new'}`, draft = drafts.get(key) || {};
        const scope = O.projects?.options('Escopo da conexão', draft.scope);
        const id = input('Identificador da conexão', edit || draft.id, 'pesquisa-local'); if (edit) id.readOnly = true;
        const transport = el('select', { class: 'input', 'aria-label': 'Tipo de conexão' }, el('option', { value: 'stdio', text: 'Servidor local' }), el('option', { value: 'http', text: 'Servidor remoto' }));
        transport.value = draft.transport || 'stdio';
        const command = input('Executável do servidor', draft.command, 'Caminho absoluto do executável');
        const argv = el('textarea', { class: 'input', rows: '3', 'aria-label': 'Argumentos em JSON', text: draft.argv || '[]' });
        const url = input('Endereço do servidor', draft.url, 'https://servidor.example/mcp');
        const secret = input('Referência da credencial no cofre', draft.secret_ref, 'ORION_MCP_PESQUISA_TOKEN');
        const accounts = api.suporta('accounts') ? await api.contas() : [];
        const account = el('select', { class: 'input', 'aria-label': 'Conta OAuth' }, el('option', { value: '', text: 'Sem conta OAuth' }), ...accounts.filter(a => a.state !== 'revoked').map(a => el('option', { value: a.id, text: `${a.name} · ${a.scope}` }))); account.value = draft.account_id || '';
        const permissions = el('textarea', { class: 'input mono', rows: '3', 'aria-label': 'Classificação das ferramentas em JSON', text: draft.permissions || '{}' });
        const trust = X.check('Revisei este comando e confio no código que será executado no computador.');
        const authorize = X.check('Autorizo o Orion a acessar este endereço remoto.');
        const local = el('div', {}, field('Executável do servidor', command), field('Argumentos em JSON', argv), trust.row);
        const remote = el('div', {}, field('Endereço do servidor', url), field('Referência da credencial no cofre', secret), field('Conta OAuth', account), X.hint('A credencial fica no cofre do Orion. Informe aqui somente o nome da referência.'), authorize.row);
        function change() { local.hidden = transport.value !== 'stdio'; remote.hidden = transport.value !== 'http'; }
        transport.addEventListener('change', change); change();
        const accepted = await X.dialog(edit ? 'Reconfigurar conexão' : 'Adicionar conexão',
            'Salvar não inicia o servidor. O teste verifica a conexão e descobre ferramentas, sem executar ações.',
            [...(scope && api.suporta('projects') ? [field('Escopo da conexão', scope)] : []), field('Identificador da conexão', id), field('Tipo de conexão', transport), local, remote,
                el('details', {}, el('summary', { text: 'Permissões de ferramentas' }), X.hint('Classifique cada ferramenta revisada: read, write, exec ou destructive. Ferramentas omitidas não ficam disponíveis ao modelo.'), field('Classificação das ferramentas em JSON', permissions))], 'Salvar configuração');
        const data = { scope: scope?.value || 'personal', id: id.value.trim(), transport: transport.value, command: command.value.trim(), argv: argv.value, url: url.value.trim(), secret_ref: secret.value.trim(), account_id: account.value || null, permissions: permissions.value };
        drafts.set(key, data);
        if (!accepted || source !== origin()) return;
        try {
            const args = JSON.parse(data.argv), classifications = JSON.parse(data.permissions);
            const config = { scope: data.scope, id: data.id, transport: data.transport, classifications, enabled: true };
            if (transport.value === 'stdio') Object.assign(config, { command: data.command, args, trusted: trust.input.checked });
            else Object.assign(config, { url: data.url, secret_ref: data.secret_ref || null, account_id: data.account_id, authorized: authorize.input.checked });
            await api.configurarMcp(config, !!edit);
            drafts.delete(key); drafts.set(`${source}:${data.id}`, data); results.delete(`${source}:${data.id}`);
            if (source === origin()) await X.refresh();
        } catch (error) { if (source === origin()) O.ui.toast(error instanceof SyntaxError ? 'Confira o JSON dos argumentos e das permissões. Sua configuração foi preservada.' : error.message, { tipo: 'erro', id: 'mcp-config' }); }
    }
    function card(row) {
        const tested = results.get(`${origin()}:${row.id}`);
        const actions = el('div', { class: 'extension-actions' });
        if (!row.id.startsWith('p_')) {
            actions.append(X.button('Testar conexão', () => X.mutate(async () => { const source = origin(); const response = await api.testarMcp(row.id); if (source === origin()) results.set(`${source}:${row.id}`, response); })),
                X.button('Reconfigurar', () => form(row.id)),
                X.button('Desativar', () => X.mutate(() => api.desativarMcp(row.id))),
                X.button('Remover', async () => { const source = origin(); if (await O.ui.confirmar({ titulo: `Remover conexão ${row.id}?`, texto: 'As ferramentas ficarão indisponíveis. Resultados anteriores são preservados.', ok: 'Remover', perigo: true }) && source === origin()) await X.mutate(() => api.removerMcp(row.id)); }, 'btn btn-ghost btn-sm'));
        }
        return el('article', { class: 'card extension-card', dataset: { connection: row.id } },
            el('div', { class: 'extension-card-head' }, el('h3', { text: row.id }), X.badge(row.state)),
            row.id.startsWith('p_') ? X.hint('Gerenciada pelo plugin. Revise ou desative na aba Plugins.') : X.hint(row.transport === 'stdio' ? 'Ferramentas executadas no computador.' : 'Ferramentas de um servidor remoto.'),
            row.error ? el('p', { class: 'banner banner-warn', text: MESSAGES[row.error] || 'Revise a configuração e teste a conexão novamente.' }) : null,
            tested ? el('p', { role: 'status', class: 'extension-hint', text: tested.state === 'connected' ? `Conexão verificada. ${tested.tools.length} ferramentas descobertas; nenhuma ação executada.` : 'A conexão ainda precisa de configuração ou autorização.' }) : null,
            el('details', {}, el('summary', { text: 'Detalhes técnicos' }), X.hint(`Transporte: ${row.transport} · Protocolo: ${row.protocol || 'não negociado'}`),
                row.error ? X.hint(`Código: ${row.error}`) : null,
                ...(tested?.tools || []).map(tool => el('p', { class: 'mono extension-capability', text: tool.name })),
                row.last_call?.possibly_active ? X.hint('Uma chamada pode ter produzido efeito antes da interrupção. Confira o destino antes de tentar novamente.') : null), actions);
    }
    X.connectionForm = () => form();
    X.connectionCard = card;
})();
