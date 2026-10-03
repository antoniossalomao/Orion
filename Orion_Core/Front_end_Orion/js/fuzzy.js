/* ==========================================================================
   ORION — fuzzy.js | busca aproximada para a paleta de comandos e filtros
   Subsequência com bônus (início de palavra, sequência, prefixo). Sem acento.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica(typeof module === 'object' ? require('./util.js') : raiz.Orion.util);
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).fuzzy = api;
})(typeof window !== 'undefined' ? window : globalThis, function (U) {
    'use strict';

    /** nota (maior = melhor) ou -1 se `consulta` não é subsequência de `texto` */
    function nota(consulta, texto) {
        const q = U.norm(consulta).replace(/\s+/g, '');
        const t = U.norm(texto);
        if (!q) return 0;
        let ti = 0, pontos = 0, seq = 0, ultimo = -2;
        for (let qi = 0; qi < q.length; qi++) {
            const c = q[qi];
            let achou = -1;
            for (let i = ti; i < t.length; i++) if (t[i] === c) { achou = i; break; }
            if (achou < 0) return -1;
            const inicioPalavra = achou === 0 || /[\s\-_/.:]/.test(t[achou - 1]);
            seq = achou === ultimo + 1 ? seq + 1 : 0;
            pontos += 1 + (inicioPalavra ? 4 : 0) + seq * 3 + (achou === qi ? 2 : 0);
            ultimo = achou;
            ti = achou + 1;
        }
        const incl = t.indexOf(q);
        if (incl >= 0) pontos += 12 + (incl === 0 ? 10 : 0);   // trecho contíguo vale muito
        return pontos - Math.min(8, t.length / 20);            // desempata para o texto mais curto
    }

    /** posições (no texto original) dos caracteres casados, para destacar na lista */
    function marcas(consulta, texto) {
        const q = U.norm(consulta).replace(/\s+/g, '');
        const t = U.norm(texto);
        if (!q || nota(consulta, texto) < 0) return [];
        const incl = t.indexOf(q);
        if (incl >= 0) return Array.from({ length: q.length }, (_, i) => incl + i);
        const pos = [];
        let ti = 0;
        for (const c of q) {
            const i = t.indexOf(c, ti);
            if (i < 0) return [];
            pos.push(i);
            ti = i + 1;
        }
        return pos;
    }

    function filtrar(itens, consulta, pegarTexto = x => x) {
        if (!String(consulta || '').trim()) return itens.slice();
        return itens
            .map(it => ({ it, n: nota(consulta, pegarTexto(it)) }))
            .filter(x => x.n >= 0)
            .sort((a, b) => b.n - a.n)
            .map(x => x.it);
    }

    /**
     * Busca por TRECHOS: todas as palavras da consulta têm de aparecer no texto (sem acento, em
     * qualquer ordem). Mais precisa que a subsequência para títulos e frases: "backup" não casa
     * com "Bot do Telegram com botões de aprovar" só porque as letras aparecem espalhadas.
     * @returns {{ok: boolean, marcas: number[], nota: number}}
     */
    function contem(consulta, texto) {
        const palavras = U.norm(consulta).split(/\s+/).filter(Boolean);
        const t = U.norm(texto);
        if (!palavras.length) return { ok: true, marcas: [], nota: 0 };
        const marcas = new Set();
        let pontos = 0;
        for (const p of palavras) {
            const i = t.indexOf(p);
            if (i < 0) return { ok: false, marcas: [], nota: -1 };
            for (let k = 0; k < p.length; k++) marcas.add(i + k);
            pontos += 10 - Math.min(9, i / 10) + (i === 0 || /[\s\-_/.:]/.test(t[i - 1]) ? 5 : 0);
        }
        return { ok: true, marcas: [...marcas].sort((a, b) => a - b), nota: pontos - Math.min(8, t.length / 20) };
    }

    function buscar(itens, consulta, pegarTexto = x => x) {
        if (!String(consulta || '').trim()) return itens.slice();
        return itens
            .map(it => ({ it, r: contem(consulta, pegarTexto(it)) }))
            .filter(x => x.r.ok)
            .sort((a, b) => b.r.nota - a.r.nota)
            .map(x => x.it);
    }

    /**
     * Todas as ocorrências de `consulta` em `texto` (sem acento, sem diferenciar maiúsculas):
     * `[[inicio, fim), …]` em posições do texto original. Para a busca dentro da conversa.
     */
    function ocorrencias(texto, consulta) {
        const q = U.norm(String(consulta ?? '').trim());
        if (!q) return [];
        const t = U.norm(texto);
        const saida = [];
        for (let i = t.indexOf(q); i >= 0; i = t.indexOf(q, i + q.length)) saida.push([i, i + q.length]);
        return saida;
    }

    return { nota, marcas, filtrar, contem, buscar, ocorrencias };
});
