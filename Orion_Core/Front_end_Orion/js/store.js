/* ==========================================================================
   ORION — store.js | estado observável pequeno, com persistência opcional
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).Store = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    /**
     * @param {object} inicial  valores padrão
     * @param {{persistir?: string[], storage?: Storage, prefixo?: string}} [opcoes]
     * Só as chaves em `persistir` vão para o storage (preferências), sempre sob try/catch:
     * o storage pode estar bloqueado ou cheio e a interface tem de funcionar igual.
     */
    function criar(inicial = {}, { persistir = [], storage = null, prefixo = 'orion_' } = {}) {
        const dados = { ...inicial };
        const ouvintes = new Map();      // chave | '*' → Set<fn>
        const lerStorage = k => {
            try { const v = storage?.getItem(prefixo + k); return v == null ? undefined : JSON.parse(v); }
            catch (_) { return undefined; }
        };
        const gravarStorage = (k, v) => {
            try { storage?.setItem(prefixo + k, JSON.stringify(v)); } catch (_) { /* sem persistência: segue */ }
        };
        for (const k of persistir) {
            const v = lerStorage(k);
            if (v !== undefined) dados[k] = v;
        }

        const avisar = (k, novo, antigo) => {
            for (const alvo of [k, '*']) {
                for (const fn of ouvintes.get(alvo) || []) {
                    try { fn(novo, antigo, k); } catch (e) { console.error('[store] ouvinte falhou', e); }
                }
            }
        };

        return {
            get: k => dados[k],
            todos: () => ({ ...dados }),
            set(k, v) {
                const antigo = dados[k];
                if (Object.is(antigo, v)) return false;
                dados[k] = v;
                if (persistir.includes(k)) gravarStorage(k, v);
                avisar(k, v, antigo);
                return true;
            },
            atualizar(k, fn) { return this.set(k, fn(dados[k])); },
            /** `chave` ou '*'; devolve a função que cancela */
            assinar(chave, fn, { imediato = false } = {}) {
                if (!ouvintes.has(chave)) ouvintes.set(chave, new Set());
                ouvintes.get(chave).add(fn);
                if (imediato && chave !== '*') fn(dados[chave], undefined, chave);
                return () => ouvintes.get(chave)?.delete(fn);
            },
        };
    }
    return { criar };
});
