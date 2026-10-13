/* ==========================================================================
   ORION — resultados_logica.js | lógica pura de E6 (sem DOM): diff, "Usou: …", assuntos
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).resultadosLogica = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    /** classe de uma linha do `unified_diff`: cabeçalho, bloco, adição, remoção ou contexto */
    function tipoLinhaDiff(linha) {
        const l = String(linha ?? '');
        if (l.startsWith('+++') || l.startsWith('---')) return 'cab';
        if (l.startsWith('@@')) return 'hunk';
        if (l[0] === '+') return 'add';
        if (l[0] === '-') return 'del';
        return 'ctx';
    }

    const ORIGENS = { nativa: 'Orion', mcp: 'MCP', skill: 'skill', plugin: 'plugin' };
    /** `mcp:github` → "MCP github"; `nativa` → "Orion"; origem desconhecida aparece como veio */
    function origemLegivel(origem) {
        const [tipo, ...resto] = String(origem || 'nativa').split(':');
        return `${ORIGENS[tipo] || tipo}${resto.length ? ' ' + resto.join(':') : ''}`;
    }

    /** só http(s); qualquer outra coisa (javascript:, data:, relativo) volta null */
    function linkSeguro(u) {
        try { const x = new URL(String(u)); return /^https?:$/.test(x.protocol) ? x.href : null; } catch (_) { return null; }
    }

    const ESTADOS = { completed: '', failed: ' · falhou', denied: ' · negada', waiting_approval: ' · aguardando aprovação', cancelled: ' · interrompida' };
    const nomeDaFerramenta = n => String(n || '').replace(/_/g, ' ');
    const linhaDeUso = x => `${nomeDaFerramenta(x.nome)} · ${origemLegivel(x.origem)}${ESTADOS[x.estado] ?? ''}`;

    /** texto do resumo do rodapé: "Usou: a, b, c +2" (ou só as fontes, se nada foi chamado) */
    function resumoDeUso(usou, nLinks = 0) {
        const nomes = [...new Set((usou || []).map(x => nomeDaFerramenta(x.nome)).filter(Boolean))];
        if (!nomes.length) return nLinks ? `Fontes: ${nLinks} link(s)` : '';
        return `Usou: ${nomes.slice(0, 3).join(', ')}${nomes.length > 3 ? ` +${nomes.length - 3}` : ''}`;
    }

    /** links únicos e seguros, no máximo `max` */
    function linksDeFontes(fontes, max = 20) {
        return [...new Set((Array.isArray(fontes) ? fontes : []).map(linkSeguro).filter(Boolean))].slice(0, max);
    }

    /** rascunho de assuntos → lista que o servidor aceita (sem vazios) ou o motivo de recusar */
    function prepararAssuntos(rascunho, max = 5, tamanho = 200) {
        const itens = (rascunho || []).map(x => String(x ?? '').replace(/\s+/g, ' ').trim()).filter(Boolean);
        if (itens.length > max) return { erro: `No máximo ${max} assuntos.` };
        if (itens.some(x => x.length > tamanho)) return { erro: `Cada assunto tem até ${tamanho} caracteres.` };
        if (itens.some(x => x.includes(';'))) return { erro: 'Assunto não pode ter ponto e vírgula.' };
        return { itens: [...new Set(itens)] };
    }

    /** "2026-10-08" → "08/10/2026", sem passar por Date (fuso não muda o dia) */
    function dataBR(iso) {
        const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(iso ?? ''));
        return m ? `${m[3]}/${m[2]}/${m[1]}` : '';
    }

    return { tipoLinhaDiff, origemLegivel, linkSeguro, linhaDeUso, resumoDeUso, linksDeFontes, prepararAssuntos, dataBR };
});
