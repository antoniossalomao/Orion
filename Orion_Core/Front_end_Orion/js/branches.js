/* Editar cria um caminho e um rascunho; só o botão Enviar inicia outra resposta. */
(function () {
    'use strict';
    const O = window.Orion, { el, api, bus, ui } = O;
    let button, generation = 0;
    async function paths() {
        const id = O.historico.sessao(), source = api.base();
        if (!id || O.chat.ocupado()) return;
        try {
            const rows = await api.caminhos(id);
            if (id !== O.historico.sessao() || source !== api.base()) return;
            const select = el('select', { class: 'input', 'aria-label': 'Caminho da conversa' }, ...rows.map((row, i) => el('option', { value: row.session_id, text: `${i ? 'Versão ' + (i + 1) : 'Original'} · ${row.title} · ${row.artifacts} resultado(s)` })));
            select.value = id;
            if (!await O.extensions.dialog('Caminhos da conversa', 'O histórico original e os resultados continuam associados à conversa que os produziu. Trocar o caminho não repete ações.', [select], 'Abrir caminho') || source !== api.base()) return;
            const row = rows.find(r => r.session_id === select.value), known = O.sidebar.sessoes().find(s => s.sessao_id === select.value);
            if (known) await O.sidebar.abrir(known); else { await api.ativarSessao(select.value); await O.historico.abrir(select.value); await O.sidebar.carregar(); }
            if (row?.edited_text && !O.$('#composer-input').value) draft(row.edited_text);
        } catch (error) { ui.toast(error.message, { tipo: 'erro' }); }
    }
    function draft(text) { const input = O.$('#composer-input'); input.value = text; input.dispatchEvent(new Event('input', { bubbles: true })); O.composer.foco(); }
    async function edit(id, session, original) {
        if (O.chat.ocupado() || O.historico.leitura() || !id || !session) return;
        const source = api.base(), text = el('textarea', { class: 'input', rows: '5', maxlength: '32000', 'aria-label': 'Pedido revisado', text: original });
        if (!await O.extensions.dialog('Editar em novo caminho', 'O pedido original fica preservado. A revisão será um rascunho em outra conversa; confira e envie quando quiser.', [text], 'Criar caminho') || source !== api.base() || session !== O.historico.sessao()) return;
        try {
            const result = await api.ramificarPedido(id, session, text.value);
            if (source !== api.base()) return;
            await O.historico.abrir(result.session_id); await O.sidebar.carregar(); draft(result.edited_text);
            O.chat.nota('Novo caminho · revise o pedido e pressione Enviar para obter outra resposta.');
        } catch (error) { ui.toast(error.message, { tipo: 'erro' }); }
    }
    bus.on('historico', () => {
        const token = ++generation;
        if (!button) { button = el('button', { type: 'button', class: 'btn btn-outline btn-sm', text: 'Caminhos', on: { click: paths } }); O.$('#chat-scroll').prepend(button); }
        button.hidden = !api.suporta('branches') || !O.historico.sessao(); button.disabled = O.chat.ocupado();
        if (token !== generation) button.hidden = true;
    });
    O.caminhos = { editar: edit };
})();
