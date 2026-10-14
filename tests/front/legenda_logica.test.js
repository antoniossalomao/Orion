const test = require('node:test');
const assert = require('node:assert/strict');
const L = require('../../Orion_Core/Front_end_Orion/js/legenda_logica.js');

test('ultimasFalas: as últimas N, sem vazios, mais recente por último', () => {
    assert.deepEqual(L.ultimasFalas(['a', '  ', 'b', 'c', 'd'], 3), ['b', 'c', 'd']);
    assert.deepEqual(L.ultimasFalas(['  oi  '], 3), ['oi']);
});

test('ultimasFalas: entrada inválida vira lista vazia', () => {
    assert.deepEqual(L.ultimasFalas(null), []);
    assert.deepEqual(L.ultimasFalas('texto'), []);
    assert.deepEqual(L.ultimasFalas([1, null, {}]), []);
});

test('estadoDaCamera: só a mensagem de câmera muda o indicador', () => {
    assert.equal(L.estadoDaCamera({ type: 'camera', on: true }, false), true);
    assert.equal(L.estadoDaCamera({ type: 'camera', on: false }, true), false);
    assert.equal(L.estadoDaCamera({ type: 'text', text: 'oi' }, true), true);
    assert.equal(L.estadoDaCamera(null, false), false);
    assert.equal(L.estadoDaCamera({ type: 'camera' }, true), false);
});

test('avisoDaCamera: texto para leitor de tela nos dois sentidos', () => {
    assert.match(L.avisoDaCamera(true), /ligada/);
    assert.match(L.avisoDaCamera(false), /desligada/);
});
