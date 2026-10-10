/* ==========================================================================
   ORION — views/arquivadas.js | conversas arquivadas: buscar, desarquivar, apagar (E3.1)
   Lista de `GET /sessoes?arquivadas=true`; a busca reusa `/sessoes/busca?arquivadas=true`. Apagar só
   existe aqui (conversa arquivada) e pede a confirmação com o título. Só `textContent`.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, api, ui } = O;
    const U = O.util;

    let itens = [], filtro = '', erro = null, geracao = 0, carregando = false;

    const titulo = s => s.titulo || 'Sem título';
    const vazio = t => el('p', { class: 'painel-vazio', role: 'status', text: t });

    async function recarregar() {
        await carregar();
        O.sidebar?.carregar?.().catch(() => {});
    }

    async function desarquivar(s) {
        try {
            await api.editarSessao(s.sessao_id, { arquivada: false });
            O.anunciar?.('Conversa restaurada.');
            ui.toast('Conversa restaurada.', { tipo: 'ok' });
            await recarregar();
        } catch (e) { ui.toast(`Não consegui restaurar: ${e.message}`, { tipo: 'erro' }); }
    }

    async function apagar(s) {
        const ok = await ui.confirmar({
            titulo: `Apagar “${titulo(s)}”?`, ok: 'Apagar para sempre', perigo: true,
            texto: 'As mensagens desta conversa são apagadas. Não dá para desfazer. Fatos e documentos da memória não são tocados.',
        });
        if (!ok) return;
        try {
            await api.apagarSessao(s.sessao_id);
            O.anunciar?.('Conversa apagada.');
            ui.toast('Conversa apagada.', { tipo: 'ok' });
            await recarregar();
        } catch (e) { ui.toast(`Não consegui apagar: ${e.message}`, { tipo: 'erro' }); }
    }

    function linha(s) {
        const quando = s.ultima_atividade ? U.quando(s.ultima_atividade) : '';
        return el('li', { class: 'painel-item arq-item', dataset: { sessao: s.sessao_id } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: titulo(s) }),
                s.project_id ? el('span', { class: 'badge badge-muted', text: 'em projeto' }) : null),
            el('small', { text: [quando, s.trecho].filter(Boolean).join(' · ') }),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Desarquivar', 'aria-label': `Desarquivar ${titulo(s)}`, on: { click: () => desarquivar(s) } }),
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar', 'aria-label': `Apagar ${titulo(s)}`, on: { click: () => apagar(s) } })));
    }

    function desenhar() {
        const raiz = $('#arquivadas-corpo');
        raiz.setAttribute('aria-busy', String(carregando));
        if (erro) { raiz.replaceChildren(vazio(`Não consegui carregar as conversas: ${erro.message}`)); return; }
        if (!itens.length) {
            raiz.replaceChildren(vazio(filtro ? `Nada encontrado para “${filtro}”.` : carregando ? 'Carregando…' : 'Nenhuma conversa arquivada. Arquive pela barra lateral (⋯ → Arquivar).'));
            return;
        }
        raiz.replaceChildren(el('ul', { class: 'painel-lista', 'aria-label': 'Conversas arquivadas' }, ...itens.map(linha)));
    }

    async function carregar() {
        const g = ++geracao; carregando = true; erro = null; desenhar();
        try {
            let d;
            if (filtro && api.suporta('session_search')) d = await api.buscarSessoes(filtro, 0, true);
            else {
                d = await api.sessoes({ arquivadas: true });
                if (filtro) d = { sessoes: O.fuzzy.buscar(d.sessoes || [], filtro, titulo) };
            }
            if (g !== geracao) return;
            itens = (d.sessoes || []).filter(s => s.arquivada);
        } catch (e) { if (g !== geracao) return; erro = e; itens = []; }
        carregando = false; desenhar();
    }

    O.views = O.views || {};
    O.views.arquivadas = {
        init() {
            const busca = $('#arquivadas-busca');
            busca.addEventListener('input', U.debounce(() => { filtro = busca.value.trim(); carregar(); }, 300));
            $('#arquivadas-refresh').addEventListener('click', carregar);
        },
        ativar() { carregar(); },
        desativar() { ++geracao; },
    };
})();
