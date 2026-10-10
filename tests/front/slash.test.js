const test = require('node:test');
const assert = require('node:assert/strict');
const S = require('../../Orion_Core/Front_end_Orion/js/slash.js');

test('/projeto aceita o nome livre e exige argumento', () => {
    assert.deepEqual(S.interpretar('/projeto Estágio'), { cmd: 'projeto', arg: 'Estágio' });
    assert.deepEqual(S.interpretar('/projeto   nenhum  '), { cmd: 'projeto', arg: 'nenhum' });
    const vazio = S.interpretar('/projeto');
    assert.equal(vazio.cmd, 'projeto');
    assert.equal(vazio.invalido, true);
});

test('/projeto aparece na lista e completa com espaço', () => {
    const [primeiro] = S.sugerir('/proj');
    assert.equal(primeiro.cmd, 'projeto');
    assert.equal(primeiro.completar, '/projeto ');
    assert.equal(primeiro.fecha, false);
});

test('comandos fixos continuam iguais', () => {
    assert.deepEqual(S.interpretar('/nova'), { cmd: 'nova' });
    assert.deepEqual(S.interpretar('/tema grafite'), { cmd: 'tema', arg: 'grafite' });
    assert.equal(S.interpretar('/tema azul').invalido, true);
    assert.equal(S.interpretar('/caminho/de/arquivo'), null);
    assert.equal(S.interpretar('texto comum'), null);
    assert.equal(S.interpretar('/xyz').desconhecido, true);
});
