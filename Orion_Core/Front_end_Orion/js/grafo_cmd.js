/* ==========================================================================
   ORION — grafo_cmd.js | comandos para andar pelo grafo da memória (puro, sem DOM)
   "me leva até X" foca um nó; "o que liga X a Y" mostra o menor caminho entre dois.
   Vale para o campo de busca da Memória, para `/grafo ...` no chat e para a voz quando a Memória
   está aberta. O nome do nó é resolvido no servidor (`/grafo/no`, `/grafo/caminho`).
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica(typeof module === 'object' ? require('./util.js') : raiz.Orion.util);
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).grafoCmd = api;
})(typeof window !== 'undefined' ? window : globalThis, function (U) {
    'use strict';

    const limpar = t => String(t ?? '').trim().replace(/[\s?!.,;:]+$/g, '').replace(/^["“'‘]+|["”'’]+$/g, '').trim();

    /** todas as formas de cortar "X a Y" no separador: nomes podem conter " a ", " e ", " com " */
    function cortes(resto, separadores) {
        const achados = [];
        const re = new RegExp(`\\s(?:${separadores})\\s`, 'gi');
        let m;
        while ((m = re.exec(resto))) {
            const de = limpar(resto.slice(0, m.index)), para = limpar(resto.slice(m.index + m[0].length));
            if (de && para) achados.push([de, para]);
            re.lastIndex = m.index + 1;
        }
        return achados;
    }

    /**
     * @returns {{tipo:'ir', alvo:string} | {tipo:'caminho', opcoes:[string,string][]} | null}
     *  `opcoes`: as formas possíveis de separar os dois nomes (quem chama tenta uma a uma).
     */
    function interpretar(texto) {
        const t = limpar(String(texto ?? '').replace(/\s+/g, ' '));
        if (!t || t.length > 300) return null;
        let m = /^o\s+que\s+(?:liga|conecta|une|relaciona)\s+(.+)$/i.exec(t);
        if (m) { const o = cortes(m[1], 'a|ao|à|e|com|ou'); return o.length ? { tipo: 'caminho', opcoes: o } : null; }
        m = /^(?:qual\s+(?:[ée]\s+)?o\s+)?caminho\s+(?:entre|de)\s+(.+)$/i.exec(t);
        if (m) { const o = cortes(m[1], 'e|para|a|at[eé]|ao'); return o.length ? { tipo: 'caminho', opcoes: o } : null; }
        m = /^como\s+(.+?)\s+(?:se\s+)?(?:liga|conecta|relaciona)\s+(?:a|ao|à|com)\s+(.+)$/i.exec(t);
        if (m) return { tipo: 'caminho', opcoes: [[limpar(m[1]), limpar(m[2])]] };
        m = /^(?:me\s+)?(?:leva|leve|levar|mostra|mostre|vai|vá|ir)\s+(?:at[eé]|para|pro|pra|a|ao|à)\s+(?:o\s+|a\s+)?(.+)$/i.exec(t)
            || /^(?:mostra|mostre)\s+(?:o\s+|a\s+)?(.+)$/i.exec(t);
        if (m && limpar(m[1])) return { tipo: 'ir', alvo: limpar(m[1]) };
        return null;
    }

    return { interpretar, cortes, limpar };
});
