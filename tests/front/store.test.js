const test = require('node:test');
const assert = require('node:assert/strict');
const { criar } = require('../../Orion_Core/Front_end_Orion/js/store.js');

const memoria = () => {
    const m = new Map();
    return { getItem: k => (m.has(k) ? m.get(k) : null), setItem: (k, v) => m.set(k, v), _m: m };
};

test('get/set e notificação só quando o valor muda', () => {
    const s = criar({ a: 1 });
    const visto = [];
    s.assinar('a', (n, o) => visto.push([n, o]));
    assert.equal(s.set('a', 1), false);
    assert.equal(s.set('a', 2), true);
    assert.deepEqual(visto, [[2, 1]]);
});

test('assinar("*") recebe qualquer chave; cancelar funciona; imediato dispara na hora', () => {
    const s = criar({ a: 1, b: 2 });
    const todos = [];
    const cancela = s.assinar('*', (n, o, k) => todos.push(k));
    s.set('a', 5); s.set('b', 6);
    cancela();
    s.set('a', 9);
    assert.deepEqual(todos, ['a', 'b']);
    let imediato = null;
    s.assinar('b', v => { imediato = v; }, { imediato: true });
    assert.equal(imediato, 6);
});

test('ouvinte que lança não derruba os outros', () => {
    const s = criar({ a: 0 });
    const original = console.error;
    console.error = () => {};
    let ok = 0;
    s.assinar('a', () => { throw new Error('x'); });
    s.assinar('a', () => { ok++; });
    s.set('a', 1);
    console.error = original;
    assert.equal(ok, 1);
});

test('persiste só as chaves pedidas e relê no próximo início', () => {
    const st = memoria();
    const s1 = criar({ tema: 'noite', temporario: 1 }, { persistir: ['tema'], storage: st });
    s1.set('tema', 'grafite'); s1.set('temporario', 2);
    assert.equal(st._m.get('orion_tema'), '"grafite"');
    assert.equal(st._m.has('orion_temporario'), false);
    const s2 = criar({ tema: 'noite' }, { persistir: ['tema'], storage: st });
    assert.equal(s2.get('tema'), 'grafite');
});

test('storage bloqueado, JSON corrompido ou ausente não quebra', () => {
    const quebrado = { getItem() { throw new Error('bloqueado'); }, setItem() { throw new Error('cheio'); } };
    const s = criar({ x: 1 }, { persistir: ['x'], storage: quebrado });
    assert.equal(s.set('x', 2), true);
    assert.equal(s.get('x'), 2);
    const st = memoria();
    st.setItem('orion_x', '{corrompido');
    assert.equal(criar({ x: 7 }, { persistir: ['x'], storage: st }).get('x'), 7);
    assert.equal(criar({ x: 7 }, { persistir: ['x'] }).get('x'), 7);
});

test('atualizar aplica função sobre o valor atual', () => {
    const s = criar({ n: 1 });
    s.atualizar('n', v => v + 1);
    assert.equal(s.get('n'), 2);
});
