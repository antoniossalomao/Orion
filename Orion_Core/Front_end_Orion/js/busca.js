/* ==========================================================================
   ORION — busca.js | busca dentro da conversa (Ctrl+F ou /buscar)
   Marca todas as ocorrências com a CSS Custom Highlight API (sem mexer no DOM das mensagens);
   onde ela não existe, seleciona a ocorrência atual. Sem acento, sem diferenciar maiúsculas.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, bus } = O;
    const U = O.util;

    const TEM_DESTAQUE = typeof CSS !== 'undefined' && !!CSS.highlights && typeof Highlight === 'function';
    let barra, entrada, contagem, col;
    let faixas = [], atual = -1, aberta = false;

    const ignorar = no => !!no.parentElement?.closest('.msg-actions, .msg-meta, .thinking, script, style');

    function varrer({ manterAtual = false } = {}) {
        const consulta = entrada.value;
        const anterior = atual;
        faixas = [];
        if (consulta.trim()) {
            const andador = document.createTreeWalker(col, NodeFilter.SHOW_TEXT, {
                acceptNode: n => (n.data.trim() && !ignorar(n) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
            });
            for (let n = andador.nextNode(); n; n = andador.nextNode()) {
                for (const [ini, fim] of O.fuzzy.ocorrencias(n.data, consulta)) {
                    const r = document.createRange();
                    r.setStart(n, ini); r.setEnd(n, Math.min(fim, n.data.length));
                    faixas.push(r);
                }
            }
        }
        if (TEM_DESTAQUE) CSS.highlights.set('busca', new Highlight(...faixas));
        atual = faixas.length ? (manterAtual && anterior >= 0 ? Math.min(anterior, faixas.length - 1) : 0) : -1;
        marcar();
    }

    function marcar() {
        if (TEM_DESTAQUE) CSS.highlights.delete('busca-atual');
        if (atual < 0) {
            contagem.textContent = entrada.value.trim() ? 'Nada encontrado' : '';
            return;
        }
        const r = faixas[atual];
        if (TEM_DESTAQUE) CSS.highlights.set('busca-atual', new Highlight(r));
        else { const s = window.getSelection(); s.removeAllRanges(); s.addRange(r); }
        r.startContainer.parentElement?.scrollIntoView({ block: 'center', behavior: O.movimentoReduzido() ? 'auto' : 'smooth' });
        contagem.textContent = `${atual + 1} de ${faixas.length}`;
    }

    function ir(delta) {
        if (!faixas.length) return;
        atual = (atual + delta + faixas.length) % faixas.length;
        marcar();
    }

    function limparMarcas() {
        faixas = []; atual = -1;
        if (TEM_DESTAQUE) { CSS.highlights.delete('busca'); CSS.highlights.delete('busca-atual'); }
    }

    function abrir(consulta) {
        if (document.documentElement.dataset.view !== 'chat') O.app.ir('chat');
        aberta = true;
        barra.hidden = false;
        if (typeof consulta === 'string') entrada.value = consulta;
        entrada.focus();
        entrada.select();
        varrer();
    }

    function fechar() {
        if (!aberta) return false;
        aberta = false;
        barra.hidden = true;
        limparMarcas();
        contagem.textContent = '';
        O.composer.foco();
        return true;
    }

    function init() {
        barra = $('#find-bar'); entrada = $('#find-input'); contagem = $('#find-count'); col = $('#chat-col');
        entrada.addEventListener('input', U.debounce(() => varrer(), 120));
        entrada.addEventListener('keydown', e => {
            if (e.key === 'Enter') { e.preventDefault(); ir(e.shiftKey ? -1 : 1); }
            else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); fechar(); }
        });
        $('#find-next').addEventListener('click', () => ir(1));
        $('#find-prev').addEventListener('click', () => ir(-1));
        $('#find-close').addEventListener('click', fechar);
        // resposta nova ou carregada: refaz a busca sem perder a posição
        bus.on('chat:ocupado', ocupado => { if (aberta && !ocupado) varrer({ manterAtual: true }); });
    }

    O.busca = { init, abrir, fechar, aberta: () => aberta, total: () => faixas.length };
})();
