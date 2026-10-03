/* ==========================================================================
   ORION — charts.js | geometria dos gráficos SVG pequenos (puro, sem DOM)
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).charts = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    const f = n => (Math.round(n * 10) / 10).toString();

    /**
     * Linha + área de uma série. `valores` pode ter nulos (lacuna). Menos de 2 pontos válidos →
     * linha plana no meio (nunca um "traço quebrado").
     * @returns {{linha: string, area: string, ultimo: {x:number,y:number}|null, min:number, max:number, n:number}}
     */
    function serie(valores, largura = 200, altura = 40, { margem = 3, min = null, max = null } = {}) {
        const v = (valores || []).map(x => (x == null || Number.isNaN(+x)) ? null : +x);
        const validos = v.filter(x => x !== null);
        if (validos.length < 2) {
            const y = altura / 2;
            return { linha: `M0 ${f(y)}L${f(largura)} ${f(y)}`, area: '', ultimo: validos.length ? { x: largura, y } : null,
                     min: validos[0] ?? 0, max: validos[0] ?? 0, n: validos.length };
        }
        const lo = min ?? Math.min(...validos), hi = max ?? Math.max(...validos);
        const span = hi - lo || 1;
        const passo = largura / (v.length - 1);
        const yDe = x => margem + (1 - (x - lo) / span) * (altura - margem * 2);
        let linha = '', area = '', primeiro = null, ultimo = null, abrir = true;
        v.forEach((x, i) => {
            if (x === null) { abrir = true; return; }
            const px = i * passo, py = yDe(x);
            linha += `${abrir ? 'M' : 'L'}${f(px)} ${f(py)}`;
            if (abrir) primeiro = { x: px, y: py };
            abrir = false;
            ultimo = { x: px, y: py };
        });
        if (primeiro && ultimo) area = `${linha}L${f(ultimo.x)} ${f(altura)}L${f(primeiro.x)} ${f(altura)}Z`;
        return { linha, area, ultimo, min: lo, max: hi, n: validos.length };
    }

    /** faixa de severidade para uso de recurso (barra de CPU/RAM/GPU) */
    function severidade(pct) {
        if (pct == null || Number.isNaN(+pct)) return 'nd';
        return pct >= 90 ? 'critico' : pct >= 75 ? 'alto' : 'ok';
    }

    /** distribuição em % → [{nome, pct, largura}] ordenada, normalizando se a soma passar de 100 */
    function barras(mapa) {
        const itens = Object.entries(mapa || {}).map(([nome, pct]) => ({ nome, pct: Math.max(0, +pct || 0) }));
        const soma = itens.reduce((s, x) => s + x.pct, 0);
        const k = soma > 100 ? 100 / soma : 1;
        return itens.sort((a, b) => b.pct - a.pct).map(x => ({ ...x, largura: Math.round(x.pct * k * 10) / 10 }));
    }

    return { serie, severidade, barras };
});
