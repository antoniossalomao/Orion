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

test('/skill e /prompt são comandos de texto livre', () => {
    assert.deepEqual(S.interpretar('/skill resumir meu texto'), { cmd: 'skill', arg: 'resumir meu texto' });
    assert.deepEqual(S.interpretar('/prompt cru:revisar texto="olá mundo"'), { cmd: 'prompt', arg: 'cru:revisar texto="olá mundo"' });
    assert.equal(S.interpretar('/skill').invalido, true);
    assert.ok(S.sugerir('/ski').some(s => s.cmd === 'skill'));
    assert.ok(S.sugerir('/pro').some(s => s.cmd === 'prompt'));
});

test('/skill separa o nome do pedido', () => {
    assert.deepEqual(S.separarSkill('resumir texto longo'), { nome: 'resumir', pedido: 'texto longo' });
    assert.deepEqual(S.separarSkill('  estilo-antonio  '), { nome: 'estilo-antonio', pedido: '' });
    assert.equal(S.separarSkill('Maiúscula x'), null);
    assert.equal(S.separarSkill(''), null);
});

test('/prompt separa servidor, nome e argumentos com ou sem aspas', () => {
    assert.deepEqual(S.separarPrompt('cru:revisar'), { servidor: 'cru', nome: 'revisar', argumentos: {} });
    assert.deepEqual(S.separarPrompt('cru:revisar texto="olá mundo" nivel=alto'),
        { servidor: 'cru', nome: 'revisar', argumentos: { texto: 'olá mundo', nivel: 'alto' } });
    assert.equal(S.separarPrompt('sem-dois-pontos'), null);
    assert.equal(S.separarPrompt('Maiusc:x'), null);
});
