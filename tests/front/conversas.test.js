const test = require('node:test');
const assert = require('node:assert/strict');
const C = require('../../Orion_Core/Front_end_Orion/js/conversas.js');

const PROJ = [{ id: 'p1', name: 'Estágio' }, { id: 'p2', name: 'TCC', archived: true }];
const S = [
    { sessao_id: 'a', project_id: null },
    { sessao_id: 'b', project_id: 'p1' },
    { sessao_id: 'c', projeto_id: 'p1' },
    { sessao_id: 'd', project_id: 'p2' },
];

test('todos, sem projeto e por projeto', () => {
    assert.equal(C.filtrar(S, 'todos', PROJ).length, 4);
    assert.deepEqual(C.filtrar(S, 'nenhum', PROJ).map(s => s.sessao_id), ['a']);
    assert.deepEqual(C.filtrar(S, 'p1', PROJ).map(s => s.sessao_id), ['b', 'c']);
});

test('filtro de projeto que não existe mais vale como todos', () => {
    assert.equal(C.filtrar(S, 'apagado', PROJ).length, 4);
    assert.equal(C.filtroValido('apagado', PROJ), 'todos');
    assert.equal(C.filtroValido('p2', PROJ), 'todos');   // arquivado
    assert.equal(C.filtroValido('p1', PROJ), 'p1');
    assert.equal(C.filtroValido(undefined, PROJ), 'todos');
    assert.equal(C.filtroValido('nenhum', []), 'nenhum');
});

test('matiz é estável e fica entre 0 e 359', () => {
    assert.equal(C.matiz('Estágio'), C.matiz('Estágio'));
    assert.notEqual(C.matiz('Estágio'), C.matiz('TCC'));
    for (const n of ['', 'a', 'Projeto muito longo 123', '日本語']) {
        const m = C.matiz(n);
        assert.ok(Number.isInteger(m) && m >= 0 && m < 360);
    }
});

test('nome curto corta com reticências e não mexe no que cabe', () => {
    assert.equal(C.nomeCurto('TCC'), 'TCC');
    assert.equal(C.nomeCurto('  Estágio   na   empresa grande ', 10), 'Estágio n…');
    assert.equal(C.nomeCurto(null), '');
});
