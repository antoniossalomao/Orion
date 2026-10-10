/* ==========================================================================
   ORION — ponte_logica.js | lógica pura das janelas da ponte de desktop (sem DOM)
   O token da ponte chega no fragmento da URL (`#t=...`): fica só neste navegador, não vai ao
   servidor nem a log. A página o lê, guarda em memória e apaga da barra de endereços.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).ponteLogica = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    /** `#t=abc&x=1` → 'abc'; vazio se não há (ou se o token parece lixo) */
    function tokenDoFragmento(hash) {
        const m = /(?:^#|&)t=([A-Za-z0-9_-]{16,200})(?:&|$)/.exec(String(hash || ''));
        return m ? m[1] : '';
    }

    /** `?modo=isso` → 'isso'; qualquer outra coisa vale 'rapido' */
    function modoDaBusca(busca) {
        const m = /(?:^\?|&)modo=([a-z]+)(?:&|$)/.exec(String(busca || ''));
        return m && ['rapido', 'isso'].includes(m[1]) ? m[1] : 'rapido';
    }

    const ROTULO = { tarefa: 'Tarefa', lembrete: 'Lembrete', gasto: 'Gasto', nota: 'Nota' };

    /** A frase mostrada depois de gravar: o que foi criado, com data/valor quando houver. */
    function resumoDaCaptura(r) {
        if (!r || !r.tipo) return 'Guardado.';
        const partes = [`${ROTULO[r.tipo] || 'Nota'}: ${r.titulo}`];
        if (r.quando) partes.push(`para ${formatarQuando(r.quando)}`);
        if (r.valor != null) partes.push(`R$ ${Number(r.valor).toFixed(2).replace('.', ',')}`);
        return partes.join(' · ');
    }

    /** '2026-10-11T15:30' → '11/10 às 15:30' */
    function formatarQuando(iso) {
        const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(String(iso || ''));
        return m ? `${m[3]}/${m[2]} às ${m[4]}:${m[5]}` : String(iso || '');
    }

    /** Mensagem humana para o erro HTTP da captura. */
    function erroDaCaptura(status, detalhe) {
        if (status === 401 || status === 403) return 'A ponte não está autorizada. Rode `orion ponte --parear` e abra de novo.';
        if (status === 409) return typeof detalhe === 'string' && detalhe ? detalhe : 'Não consegui guardar agora.';
        if (status === 422) return 'Texto vazio ou longo demais (até 2000 caracteres).';
        return `O servidor não respondeu (${status || 'sem conexão'}).`;
    }

    return { tokenDoFragmento, modoDaBusca, resumoDaCaptura, formatarQuando, erroDaCaptura };
});
