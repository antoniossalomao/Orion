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

    // só o que a pessoa vê numa mensagem: nada de rótulos, ações, "pensando", aviso de demora ou estado vazio
    const ignorar = no => !!no.parentElement?.closest('.msg-actions, .msg-meta, .thinking, .slow-note, script, style');

    function varrer({ manterAtual = false } = {}) {
        const consulta = entrada.value;
        const anterior = atual;
        faixas = [];
        if (consulta.trim()) {
            for (const raiz of col.querySelectorAll('.msg, .msg-system')) {      // o estado vazio (#chat-empty) fica de fora
                const andador = document.createTreeWalker(raiz, NodeFilter.SHOW_TEXT, {
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
        if (!r.startContainer.isConnected) { varrer({ manterAtual: true }); return; }   // o texto foi re-renderizado
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

    function mostrar(consulta) {
        aberta = true;
        barra.hidden = false;
        if (typeof consulta === 'string') entrada.value = consulta;
        entrada.focus();
        entrada.select();
        varrer();
    }
    function abrir(consulta) {
        if (document.documentElement.dataset.view === 'chat') { mostrar(consulta); return; }
        // a troca de tela é assíncrona (hash): só mostra e foca quando o chat já está ativo, e sem a tela devolver o foco ao campo
        const solta = bus.on('view', () => { solta(); mostrar(consulta); });
        O.app.ir('chat', { foco: false });
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
        // resposta nova, conversa trocada, /limpar, re-render do streaming: refaz a busca (no máx. a cada 150 ms)
        const refazer = U.debounce(() => { if (aberta) varrer({ manterAtual: true }); }, 150);
        new MutationObserver(refazer).observe(col, { childList: true, subtree: true, characterData: true });
    }

    O.busca = { init, abrir, fechar, aberta: () => aberta, total: () => faixas.length };
})();
