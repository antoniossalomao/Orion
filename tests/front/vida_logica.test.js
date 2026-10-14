const test = require('node:test');
const assert = require('node:assert/strict');
const L = require('../../Orion_Core/Front_end_Orion/js/vida_logica.js');

test('reais: formato brasileiro, milhar e sinal', () => {
    assert.equal(L.reais(123456), 'R$ 1.234,56');
    assert.equal(L.reais(5), 'R$ 0,05');
    assert.equal(L.reais(0), 'R$ 0,00');
    assert.equal(L.reais(-250), '-R$ 2,50');
    assert.equal(L.reais(100000000), 'R$ 1.000.000,00');
});

test('agrupa a linha do tempo por dia local, do mais novo ao mais antigo', () => {
    const dia1 = new Date(2026, 9, 13, 9).getTime() / 1000;
    const dia2 = new Date(2026, 9, 14, 18).getTime() / 1000;
    const dia2b = new Date(2026, 9, 14, 8).getTime() / 1000;
    const grupos = L.agrupaPorDia([
        { ts: dia1, tipo: 'gasto', texto: 'a' },
        { ts: dia2, tipo: 'tela', texto: 'b' },
        { ts: dia2b, tipo: 'conversa', texto: 'c' },
    ]);
    assert.deepEqual(grupos.map(g => g.dia), ['2026-10-14', '2026-10-13']);
    assert.deepEqual(grupos[0].itens.map(i => i.texto), ['b', 'c']);
});

test('rótulos, faixas e janela de zoom', () => {
    assert.equal(L.rotuloTipo('area'), 'área de transferência');
    assert.equal(L.rotuloTipo('x'), 'x');
    assert.equal(L.faixas({ p10: 100, p50: 200, p90: 300 }),
        'pessimista R$ 1,00 · típico R$ 2,00 · otimista R$ 3,00');
    assert.deepEqual(L.janela('dia', 1000), { de: 1000 - 86400, ate: 1000 });
    assert.deepEqual(L.janela('semana', 1000), { de: 1000 - 7 * 86400, ate: 1000 });
});
