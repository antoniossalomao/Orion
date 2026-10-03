/* ==========================================================================
   ORION — sse.js | parser de Server-Sent Events e normalização dos eventos do /chat
   Contrato do backend (legado e orion.app): linhas `data: {...}` e `data: [DONE]`.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).sse = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    /**
     * Parser incremental: `alimentar(pedaco)` aceita texto em qualquer corte (um evento pode
     * chegar partido entre dois `read()` do fetch). Chama `aoEvento(objeto | '[DONE]')`.
     */
    function criarParser(aoEvento) {
        let resto = '';
        let dados = [];
        const despachar = () => {
            if (!dados.length) return;
            const bruto = dados.join('\n');
            dados = [];
            if (bruto === '[DONE]') { aoEvento('[DONE]'); return; }
            try { aoEvento(JSON.parse(bruto)); }
            catch (_) { /* linha que não é JSON (keep-alive, lixo): ignora */ }
        };
        return {
            alimentar(pedaco) {
                resto += pedaco;
                let i;
                while ((i = resto.search(/\r\n|\n|\r/)) >= 0) {
                    const linha = resto.slice(0, i);
                    const sep = resto.slice(i).match(/^(\r\n|\n|\r)/)[0];
                    // "\r" no fim do buffer pode ser o começo de "\r\n": espera o próximo pedaço
                    if (sep === '\r' && i + 1 === resto.length) return;
                    resto = resto.slice(i + sep.length);
                    if (linha === '') despachar();
                    else if (linha.startsWith('data:')) dados.push(linha.slice(5).replace(/^ /, ''));
                    // outros campos (event:, id:, retry:, comentários ':') não são usados
                }
            },
            fim() { if (resto.startsWith('data:')) { dados.push(resto.slice(5).replace(/^ /, '')); resto = ''; } despachar(); },
        };
    }

    /**
     * Normaliza um objeto do SSE para um evento interno do chat:
     *   {tipo:'modelo', nome} · {tipo:'texto', texto} · {tipo:'ferramenta', ...}
     *   {tipo:'aprovacao', ...} · {tipo:'erro', mensagem} · {tipo:'fim'} · null (desconhecido)
     */
    function normalizar(ev) {
        if (ev === '[DONE]') return { tipo: 'fim' };
        if (!ev || typeof ev !== 'object') return null;
        if (typeof ev.text === 'string' && ev.text) return { tipo: 'texto', texto: ev.text };
        if (typeof ev.tier === 'string' && ev.tier) return { tipo: 'modelo', nome: ev.tier };
        if (ev.tool && typeof ev.tool === 'object') {
            const t = ev.tool;
            return { tipo: 'ferramenta', nome: String(t.name || ''), decisao: t.decision || null,
                     motivo: t.reason || '', aprovada: !!t.approved, erro: t.error || null };
        }
        if (ev.approval && typeof ev.approval === 'object') {
            const a = ev.approval;
            return { tipo: 'aprovacao', id: String(a.id || ''), ferramenta: String(a.tool || ''),
                     motivo: a.reason || '', args: a.args && typeof a.args === 'object' ? a.args : {} };
        }
        if (typeof ev.error === 'string') return { tipo: 'erro', mensagem: ev.error };
        return null;
    }

    return { criarParser, normalizar };
});
