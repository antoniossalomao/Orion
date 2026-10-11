/* Histórico paginado; a sessão visualizada pode ser diferente da sessão ativa. */
(function () {
    'use strict';
    const O = window.Orion, { api, el, ui, bus } = O;
    let sid = null, leitura = false, completo = false, cursor = null, geracao = 0, carregando = false, barra, anterioresBtn, registroBtn, origem = null, caixaResumo = null;
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
    /** auto-compact (regra 53): quando o começo da conversa foi resumido, avisa e deixa ver/descartar o resumo */
    async function mostrarResumo(id) {
        if (!caixaResumo) { caixaResumo = el('div', { class: 'history-resumo', role: 'status', hidden: true }); O.$('#chat-scroll').prepend(caixaResumo); }
        caixaResumo.hidden = true; caixaResumo.replaceChildren();
        if (!id || !api.suporta('sessions') || api.estado().backend !== 'orion') return;
        let r = null;
        try { r = (await api.resumoSessao(id)).resumo; } catch (_) { return; }
        if (!r || sid !== id) return;
        const texto = el('pre', { class: 'history-resumo-texto selectable', hidden: true, text: r.texto });
        const ver = el('button', { type: 'button', class: 'btn btn-ghost btn-sm', text: 'ver resumo', 'aria-expanded': 'false', on: { click: () => { texto.hidden = !texto.hidden; ver.setAttribute('aria-expanded', String(!texto.hidden)); ver.textContent = texto.hidden ? 'ver resumo' : 'esconder resumo'; } } });
        const descartar = el('button', { type: 'button', class: 'btn btn-ghost btn-sm', text: 'descartar', on: { click: async () => { try { await api.descartarResumo(id); caixaResumo.hidden = true; caixaResumo.replaceChildren(); } catch (e) { ui.toast(`Não consegui descartar o resumo: ${e.message}`, { tipo: 'erro' }); } } } });
        caixaResumo.replaceChildren(el('span', { text: `Conversa resumida até a mensagem ${r.ate_mensagem} (gerado pelo modelo local, pode ter erro; o registro não muda). ` }), ver, descartar, texto);
        caixaResumo.hidden = false;
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
            mostrarResumo(registro ? null : id);
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
    bus.on('chat:ocupado', () => { if (barra) controles(); if (sid && !O.chat.ocupado() && !completo) setTimeout(() => sid && mostrarResumo(sid), 1500); });
    O.historico = { abrir, sessao: () => sid, completo: () => completo, somenteLeitura: () => leitura, leitura: () => leitura || carregando || O.sidebar?.abrindo() };
})();
