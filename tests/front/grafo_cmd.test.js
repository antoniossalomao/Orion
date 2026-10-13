const test = require('node:test');
const assert = require('node:assert/strict');
const { interpretar } = require('../../Orion_Core/Front_end_Orion/js/grafo_cmd.js');

test('"me leva até X" foca um nó', () => {
    for (const frase of ['me leva até Orion', 'Me leva ate Orion', 'leva até o Orion', 'mostra Orion', 'vai para Orion', 'Me leve até "Orion"!']) {
        assert.deepEqual(interpretar(frase), { tipo: 'ir', alvo: 'Orion' }, frase);
    }
    assert.deepEqual(interpretar('me leva até a Memória Híbrida?'), { tipo: 'ir', alvo: 'Memória Híbrida' });
});

test('"o que liga X a Y" devolve as formas de separar os nomes', () => {
    assert.deepEqual(interpretar('o que liga Orion a MCP?'), { tipo: 'caminho', opcoes: [['Orion', 'MCP']] });
    assert.deepEqual(interpretar('O que liga RAG e MCP'), { tipo: 'caminho', opcoes: [['RAG', 'MCP']] });
    assert.deepEqual(interpretar('o que conecta Unimar com Marília'), { tipo: 'caminho', opcoes: [['Unimar', 'Marília']] });
});

test('nomes com " a " dentro dão várias opções, da esquerda para a direita', () => {
    const r = interpretar('o que liga Busca a Frio a Memória');
    assert.equal(r.tipo, 'caminho');
    assert.deepEqual(r.opcoes, [['Busca', 'Frio a Memória'], ['Busca a Frio', 'Memória']]);
});

test('outras formas de pedir o caminho', () => {
    assert.deepEqual(interpretar('caminho entre RAG e Orion'), { tipo: 'caminho', opcoes: [['RAG', 'Orion']] });
    assert.deepEqual(interpretar('qual o caminho de RAG para Orion'), { tipo: 'caminho', opcoes: [['RAG', 'Orion']] });
    assert.deepEqual(interpretar('como o RAG se liga ao Orion?'), { tipo: 'caminho', opcoes: [['o RAG', 'Orion']] });
});

test('o que não é comando do grafo vira null', () => {
    for (const frase of ['', '   ', 'oi, tudo bem?', 'o que liga', 'o que liga Orion', 'me leva', 'leva até', 'x'.repeat(400)]) {
        assert.equal(interpretar(frase), null, frase);
    }
});
