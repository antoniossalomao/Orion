/* ==========================================================================
   ORION — vida_logica.js | lógica pura da Vida pessoal (E10), sem DOM
   Formatação em reais, agrupamento da linha do tempo por dia e rótulos. Tudo testável em Node.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else raiz.OrionVidaLogica = api;
})(typeof self !== 'undefined' ? self : this, function () {
    'use strict';

    /** centavos → "R$ 1.234,56" (formato brasileiro; negativo com o sinal na frente) */
    function reais(cents) {
        const n = Number(cents) || 0;
        const sinal = n < 0 ? '-' : '';
        const abs = Math.abs(Math.round(n));
        const inteiro = Math.floor(abs / 100).toString().replace(/\B(?=(\d{3})+(?!\d))/g, '.');
        const centavos = String(abs % 100).padStart(2, '0');
        return `${sinal}R$ ${inteiro},${centavos}`;
    }

    /** data local AAAA-MM-DD de um epoch em segundos (o dia é o seu, não o do UTC) */
    function diaLocal(ts) {
        const d = new Date(Number(ts) * 1000);
        const m = String(d.getMonth() + 1).padStart(2, '0');
        const dia = String(d.getDate()).padStart(2, '0');
        return `${d.getFullYear()}-${m}-${dia}`;
    }

    /** itens da linha do tempo agrupados por dia, do mais novo para o mais antigo */
    function agrupaPorDia(itens) {
        const grupos = new Map();
        for (const it of itens || []) {
            const dia = diaLocal(it.ts);
            if (!grupos.has(dia)) grupos.set(dia, []);
            grupos.get(dia).push(it);
        }
        return [...grupos.entries()]
            .sort(([a], [b]) => (a < b ? 1 : a > b ? -1 : 0))
            .map(([dia, lista]) => ({ dia, itens: lista.sort((x, y) => y.ts - x.ts) }));
    }

    const ROTULOS = {
        conversa: 'conversa', gasto: 'gasto', tela: 'tela', area: 'área de transferência',
        capsula: 'cápsula', documento: 'documento', diario: 'diário', commit: 'commit',
    };
    function rotuloTipo(tipo) { return ROTULOS[tipo] || String(tipo || 'outro'); }

    /** "pessimista / típico / otimista" a partir das faixas de cenário (centavos) */
    function faixas(f) {
        return `pessimista ${reais(f.p10)} · típico ${reais(f.p50)} · otimista ${reais(f.p90)}`;
    }

    /** a janela da linha do tempo: dia ou semana, terminando agora (segundos) */
    function janela(zoom, agora) {
        const dias = zoom === 'dia' ? 1 : 7;
        return { de: Math.floor(agora - dias * 86400), ate: Math.floor(agora) };
    }

    return { reais, diaLocal, agrupaPorDia, rotuloTipo, faixas, janela };
});
