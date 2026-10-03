const test = require('node:test');
const assert = require('node:assert/strict');
const C = require('../../Orion_Core/Front_end_Orion/js/charts.js');

test('série: linha e área para dados normais', () => {
    const s = C.serie([1, 3, 2, 5], 90, 30, { margem: 0 });
    assert.equal(s.linha, 'M0 30L30 15L60 22.5L90 0');
    assert.equal(s.area, 'M0 30L30 15L60 22.5L90 0L90 30L0 30Z');
    assert.deepEqual(s.ultimo, { x: 90, y: 0 });
    assert.deepEqual([s.min, s.max, s.n], [1, 5, 4]);
});

test('série: menos de 2 pontos vira linha plana, nunca um traço quebrado', () => {
    for (const v of [[], [7], [null, null], null]) {
        const s = C.serie(v, 200, 40);
        assert.match(s.linha, /^M0 20L200 20$/);
        assert.equal(s.area, '');
    }
    assert.deepEqual(C.serie([7]).ultimo, { x: 200, y: 20 });
    assert.equal(C.serie([]).ultimo, null);
});

test('série: lacunas (null) interrompem a linha', () => {
    const s = C.serie([1, null, 3, 4], 90, 30, { margem: 0 });
    assert.equal((s.linha.match(/M/g) || []).length, 2);
    assert.equal(s.n, 3);
});

test('série: valores constantes não dividem por zero', () => {
    const s = C.serie([5, 5, 5], 100, 20);
    assert.doesNotMatch(s.linha, /NaN|Infinity/);
});

test('severidade por faixa de uso', () => {
    assert.deepEqual([10, 74.9, 75, 89, 90, 100].map(C.severidade), ['ok', 'ok', 'alto', 'alto', 'critico', 'critico']);
    assert.equal(C.severidade(null), 'nd');
    assert.equal(C.severidade('x'), 'nd');
});

test('barras: ordena, normaliza soma acima de 100 e ignora lixo', () => {
    const b = C.barras({ Groq: 71.1, Gemini: 22.7, Claude: 6.2 });
    assert.deepEqual(b.map(x => x.nome), ['Groq', 'Gemini', 'Claude']);
    const c = C.barras({ a: 80, b: 80 });
    assert.equal(c[0].largura + c[1].largura, 100);
    assert.deepEqual(C.barras(null), []);
    assert.equal(C.barras({ x: 'lixo' })[0].pct, 0);
});
