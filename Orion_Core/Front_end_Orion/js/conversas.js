/* ==========================================================================
   ORION — conversas.js | lógica pura da lista de conversas por projeto (sem DOM)
   Filtro da barra lateral (`todos` · `nenhum` · id do projeto), cor e nome curto do selo.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).conversas = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    const TODOS = 'todos', NENHUM = 'nenhum';

    /** o id do projeto da conversa, aceitando os dois nomes que os backends usam */
    const projetoDe = s => s?.project_id ?? s?.projeto_id ?? null;

    /** Mantém as conversas do filtro. Filtro desconhecido (projeto apagado) vale como `todos`. */
    function filtrar(sessoes, filtro, projetos = []) {
        if (!filtro || filtro === TODOS) return sessoes;
        if (filtro === NENHUM) return sessoes.filter(s => !projetoDe(s));
        if (!projetos.some(p => p.id === filtro)) return sessoes;
        return sessoes.filter(s => projetoDe(s) === filtro);
    }

    /** o filtro que de fato vale: o salvo, se o projeto ainda existe e não está arquivado */
    function filtroValido(filtro, projetos = []) {
        if (!filtro || filtro === TODOS || filtro === NENHUM) return filtro || TODOS;
        return projetos.some(p => p.id === filtro && !p.archived) ? filtro : TODOS;
    }

    /** matiz 0–359 estável para o nome (hash de string); a mesma cor em toda sessão */
    function matiz(nome) {
        let h = 0;
        for (const c of String(nome ?? '')) h = (h * 31 + c.codePointAt(0)) >>> 0;
        return h % 360;
    }

    /** nome curto para o selo: até `max` caracteres, com reticências */
    function nomeCurto(nome, max = 10) {
        const t = String(nome ?? '').trim().replace(/\s+/g, ' ');
        return t.length > max ? `${t.slice(0, max - 1).trimEnd()}…` : t;
    }

    return { TODOS, NENHUM, projetoDe, filtrar, filtroValido, matiz, nomeCurto };
});
