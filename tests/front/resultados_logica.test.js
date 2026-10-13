const test = require('node:test');
const assert = require('node:assert/strict');
const L = require('../../Orion_Core/Front_end_Orion/js/resultados_logica.js');

test('linhas do diff: cabeçalho, bloco, adição, remoção e contexto', () => {
    assert.equal(L.tipoLinhaDiff('--- base'), 'cab');
    assert.equal(L.tipoLinhaDiff('+++ novo'), 'cab');
    assert.equal(L.tipoLinhaDiff('@@ -1,3 +1,4 @@'), 'hunk');
    assert.equal(L.tipoLinhaDiff('+linha nova'), 'add');
    assert.equal(L.tipoLinhaDiff('-linha velha'), 'del');
    assert.equal(L.tipoLinhaDiff(' igual'), 'ctx');
    assert.equal(L.tipoLinhaDiff(''), 'ctx');
});

test('origem legível: nativa, mcp, skill, plugin e desconhecida', () => {
    assert.equal(L.origemLegivel('nativa'), 'Orion');
    assert.equal(L.origemLegivel(undefined), 'Orion');
    assert.equal(L.origemLegivel('mcp:github'), 'MCP github');
    assert.equal(L.origemLegivel('skill:estilo-antonio'), 'skill estilo-antonio');
    assert.equal(L.origemLegivel('plugin:pesquisa'), 'plugin pesquisa');
    assert.equal(L.origemLegivel('algo:x:y'), 'algo x:y');
});

test('links: só http(s), sem repetir, no máximo o teto', () => {
    assert.equal(L.linkSeguro('https://a.com/x'), 'https://a.com/x');
    assert.equal(L.linkSeguro('javascript:alert(1)'), null);
    assert.equal(L.linkSeguro('data:text/html,<b>'), null);
    assert.equal(L.linkSeguro('/relativo'), null);
    assert.equal(L.linkSeguro(null), null);
    assert.deepEqual(L.linksDeFontes(['https://a.com/', 'https://a.com/', 'ftp://x', 'http://b.org']), ['https://a.com/', 'http://b.org/']);
    assert.deepEqual(L.linksDeFontes('não é lista'), []);
    assert.equal(L.linksDeFontes(Array.from({ length: 50 }, (_, i) => `https://s${i}.com`)).length, 20);
});

test('"Usou: …" resume até 3 nomes e conta o resto', () => {
    const u = n => ({ nome: n, origem: 'nativa', estado: 'completed' });
    assert.equal(L.resumoDeUso([u('buscar_url'), u('ler_arquivo')]), 'Usou: buscar url, ler arquivo');
    assert.equal(L.resumoDeUso([u('a'), u('b'), u('c'), u('d'), u('e')]), 'Usou: a, b, c +2');
    assert.equal(L.resumoDeUso([u('a'), u('a')]), 'Usou: a');
    assert.equal(L.resumoDeUso([], 2), 'Fontes: 2 link(s)');
    assert.equal(L.resumoDeUso([], 0), '');
});

test('linha de uso mostra a origem e o estado quando não deu certo', () => {
    assert.equal(L.linhaDeUso({ nome: 'buscar_url', origem: 'mcp:docs', estado: 'completed' }), 'buscar url · MCP docs');
    assert.equal(L.linhaDeUso({ nome: 'apagar_arquivo', origem: 'nativa', estado: 'denied' }), 'apagar arquivo · Orion · negada');
    assert.equal(L.linhaDeUso({ nome: 'x', origem: 'nativa', estado: 'waiting_approval' }), 'x · Orion · aguardando aprovação');
});

test('assuntos: tira vazios, junta espaços, tira repetidos e recusa excesso', () => {
    assert.deepEqual(L.prepararAssuntos(['  zkML  ', '', 'Pix   e  Open Finance', 'zkML']), { itens: ['zkML', 'Pix e Open Finance'] });
    assert.deepEqual(L.prepararAssuntos([]), { itens: [] });
    assert.match(L.prepararAssuntos(['a', 'b', 'c', 'd', 'e', 'f']).erro, /No máximo 5/);
    assert.match(L.prepararAssuntos(['x'.repeat(201)]).erro, /200 caracteres/);
    assert.match(L.prepararAssuntos(['a;b']).erro, /ponto e vírgula/);
    assert.deepEqual(L.prepararAssuntos(['a', 'b'], 2, 5), { itens: ['a', 'b'] });
});

test('data BR não muda de dia por causa do fuso', () => {
    assert.equal(L.dataBR('2026-10-08'), '08/10/2026');
    assert.equal(L.dataBR('2026-10-08T23:59:00+00:00'), '08/10/2026');
    assert.equal(L.dataBR(''), '');
    assert.equal(L.dataBR(undefined), '');
});
