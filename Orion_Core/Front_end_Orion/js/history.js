/* Histórico paginado; a sessão visualizada pode ser diferente da sessão ativa. */
(function () {
    'use strict';
    const O = window.Orion, { api, el, ui, bus } = O;
    let sid = null, leitura = false, completo = false, cursor = null, geracao = 0, carregando = false, barra, anterioresBtn, registroBtn, origem = null;
    function controles() {
        if (!barra) {
            anterioresBtn = el('button', { type: 'button', class: 'btn btn-outline btn-sm', on: { click: anteriores } });
            registroBtn = el('button', { type: 'button', class: 'btn btn-outline btn-sm', on: { click: () => abrir(sid, !completo).catch(e => ui.toast(`Não consegui abrir o registro: ${e.message}`, { tipo: 'erro' })) } });
            barra = el('div', { class: 'history-controls', role: 'group', 'aria-label': 'Histórico da conversa' }, anterioresBtn, registroBtn);
            O.$('#chat-scroll').prepend(barra);
        }
        anterioresBtn.hidden = !cursor;
        anterioresBtn.disabled = carregando || O.chat.ocupado();
        anterioresBtn.textContent = carregando ? 'Carregando…' : 'Carregar mensagens anteriores';
        registroBtn.textContent = completo ? 'Voltar à conversa atual' : 'Ver registro completo';
        registroBtn.disabled = carregando || O.chat.ocupado();
        barra.hidden = !sid;
    }
    async function abrir(id, registro = false) {
        const g = ++geracao;
        carregando = true;
        bus.emit('historico');
        controles();
        try {
            const d = await api.historico(id, { completo: registro });
            if (g !== geracao) return;
            sid = id; completo = registro; leitura = registro || d.somente_leitura;
            cursor = d.proximo_antes;
            O.chat.limpar(); O.chat.renderHistorico(d.mensagens);
            if (leitura) O.chat.nota('Registro da conversa · somente leitura.');
        } catch (e) { if (g === geracao) leitura = true; throw e; }
        finally { if (g === geracao) { carregando = false; controles(); bus.emit('historico'); } }
    }
    async function anteriores() {
        if (!cursor || carregando || O.chat.ocupado()) return;
        const g = geracao;
        carregando = true; controles(); bus.emit('historico');
        try {
            const d = await api.historico(sid, { antes: cursor, completo });
            if (g !== geracao) return;
            O.chat.renderHistorico(d.mensagens, { antes: true }); cursor = d.proximo_antes;
        } catch (e) { ui.toast(`Não consegui carregar o histórico: ${e.message}`, { tipo: 'erro' }); }
        finally { if (g === geracao) { carregando = false; controles(); bus.emit('historico'); } }
    }
    bus.on('capabilities', () => {
        const chave = `${api.base()}:${api.estado().backend}`;
        if (origem !== chave) { origem = chave; ++geracao; sid = null; leitura = false; completo = false; cursor = null; carregando = false; if (barra) controles(); bus.emit('historico'); }
    });
    bus.on('chat:ocupado', () => { if (barra) controles(); });
    O.historico = { abrir, sessao: () => sid, completo: () => completo, leitura: () => leitura || carregando || O.sidebar?.abrindo() };
})();
