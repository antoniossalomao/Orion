const test = require('node:test');
const assert = require('node:assert/strict');
const { escolherMime, eventosDaMensagem, mensagemDeAuth } = require('../../Orion_Core/Front_end_Orion/js/fala_core.js');
const { normalizar } = require('../../Orion_Core/Front_end_Orion/js/sse.js');

test('escolhe o primeiro formato que o navegador grava', () => {
    assert.equal(escolherMime(() => true), 'audio/webm;codecs=opus');
    assert.equal(escolherMime(m => m === 'audio/mp4'), 'audio/mp4');           // Safari
    assert.equal(escolherMime(() => false), null);
    assert.equal(escolherMime(() => { throw new Error('sem isTypeSupported'); }), null);
});

test('heard abre o turno e mostra o que o Orion entendeu', () => {
    assert.deepEqual(eventosDaMensagem({ type: 'heard', text: '  que horas são ' }, normalizar),
        [{ tipo: 'inicio' }, { tipo: 'usuario', texto: 'que horas são' }]);
    assert.deepEqual(eventosDaMensagem({ type: 'heard', text: '   ' }, normalizar), []);
});

test('ev reaproveita a normalização do /chat (texto, ferramenta, aprovação)', () => {
    assert.deepEqual(eventosDaMensagem({ type: 'ev', ev: { text: 'Olá' } }, normalizar), [{ tipo: 'texto', texto: 'Olá' }]);
    const [ap] = eventosDaMensagem({ type: 'ev', ev: { approval: { id: 'a1', tool: 'esquecer_fato', args: { id: 1 } } } }, normalizar);
    assert.equal(ap.tipo, 'aprovacao');
    assert.equal(ap.id, 'a1');
    assert.deepEqual(eventosDaMensagem({ type: 'ev', ev: { lixo: 1 } }, normalizar), []);
});

test('erro e fim do turno', () => {
    assert.deepEqual(eventosDaMensagem({ type: 'error', msg: 'fala longa demais' }, normalizar), [{ tipo: 'erro', mensagem: 'fala longa demais' }]);
    assert.equal(eventosDaMensagem({ type: 'error' }, normalizar)[0].tipo, 'erro');
    assert.deepEqual(eventosDaMensagem({ type: 'done' }, normalizar), [{ tipo: 'fim' }]);
});

test('audio, tipos desconhecidos e lixo não geram evento', () => {
    for (const m of [{ type: 'audio', mime: 'audio/mpeg' }, { type: 'novo' }, null, 'x', 42]) {
        assert.deepEqual(eventosDaMensagem(m, normalizar), []);
    }
});

test('o token vai na primeira mensagem, nunca na URL; sem token não manda nada', () => {
    assert.deepEqual(JSON.parse(mensagemDeAuth('t0k')), { cmd: 'auth', token: 't0k' });
    assert.equal(mensagemDeAuth(''), null);
    assert.equal(mensagemDeAuth(undefined), null);
});

test('floatParaPcm16: escala, satura e acha o pico', () => {
    const { floatParaPcm16 } = require('../../Orion_Core/Front_end_Orion/js/fala_core.js');
    const { pcm, pico } = floatParaPcm16(new Float32Array([0, 0.5, -0.5, 1, -1, 2, -3]));
    assert.deepEqual(Array.from(pcm), [0, 16383, -16384, 32767, -32768, 32767, -32768]);
    assert.equal(pico, 1);
    assert.equal(floatParaPcm16(new Float32Array(0)).pico, 0);
});
