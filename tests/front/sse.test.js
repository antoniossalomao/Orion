const test = require('node:test');
const assert = require('node:assert/strict');
const { criarParser, normalizar } = require('../../Orion_Core/Front_end_Orion/js/sse.js');

function coletar(texto, corte = null) {
    const evs = [];
    const p = criarParser(e => evs.push(e));
    if (corte === null) p.alimentar(texto);
    else for (let i = 0; i < texto.length; i += corte) p.alimentar(texto.slice(i, i + corte));
    p.fim();
    return evs;
}

const STREAM = 'data: {"tier": "Groq"}\n\ndata: {"text": "Olá, "}\n\ndata: {"text": "Antônio."}\n\ndata: [DONE]\n\n';
const ESPERADO = [{ tier: 'Groq' }, { text: 'Olá, ' }, { text: 'Antônio.' }, '[DONE]'];

test('parseia o formato do /chat', () => {
    assert.deepEqual(coletar(STREAM), ESPERADO);
});

test('o resultado não depende de onde o stream foi cortado (1 a 50 bytes por pedaço)', () => {
    for (let n = 1; n <= 50; n++) assert.deepEqual(coletar(STREAM, n), ESPERADO, `corte ${n}`);
});

test('aceita \\r\\n, \\r e dados multilinha; ignora comentários, event:, id: e lixo', () => {
    assert.deepEqual(coletar('data: {"a":1}\r\n\r\n'), [{ a: 1 }]);
    assert.deepEqual(coletar('data: {"a":1}\r\r'), [{ a: 1 }]);
    assert.deepEqual(coletar('data: {"a":\ndata: 2}\n\n'), [{ a: 2 }]);
    assert.deepEqual(coletar(': keep-alive\n\nevent: x\nid: 7\ndata: {"b":1}\n\ndata: nao-json\n\n'), [{ b: 1 }]);
});

test('o último evento sem linha em branco final sai no fim()', () => {
    assert.deepEqual(coletar('data: {"text":"x"}'), [{ text: 'x' }]);
});

test('normalizar: eventos do legado e do orion.app', () => {
    assert.deepEqual(normalizar('[DONE]'), { tipo: 'fim' });
    assert.deepEqual(normalizar({ text: 'oi' }), { tipo: 'texto', texto: 'oi' });
    assert.deepEqual(normalizar({ tier: 'Groq' }), { tipo: 'modelo', nome: 'Groq' });
    assert.deepEqual(normalizar({ tool: { name: 'buscar_memoria', decision: 'allow', reason: '' } }),
        { tipo: 'ferramenta', nome: 'buscar_memoria', decisao: 'allow', motivo: '', aprovada: false, erro: null });
    assert.deepEqual(normalizar({ approval: { id: 'a1', tool: 'executar_comando', reason: 'r', args: { cmd: 'x' } } }),
        { tipo: 'aprovacao', id: 'a1', ferramenta: 'executar_comando', motivo: 'r', args: { cmd: 'x' } });
    assert.deepEqual(normalizar({ error: 'falhou' }), { tipo: 'erro', mensagem: 'falhou' });
    for (const lixo of [null, undefined, 3, {}, { text: '' }, { tier: '' }, { tool: 'x' }, { approval: 3 }]) {
        assert.equal(normalizar(lixo), null);
    }
});
