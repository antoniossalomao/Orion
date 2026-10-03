const test = require('node:test');
const assert = require('node:assert/strict');
const F = require('../../Orion_Core/Front_end_Orion/js/fuzzy.js');

const CMDS = ['Nova conversa', 'Ir para Início', 'Ir para Memória', 'Ir para Configurações', 'Exportar conversa',
              'Alternar resposta por voz', 'Mudar tema', 'Limpar histórico'];

test('subsequência, sem acento e sem diferença de caixa', () => {
    assert.ok(F.nota('conf', 'Configurações') > 0);
    assert.ok(F.nota('MEMO', 'Ir para Memória') > 0);
    assert.ok(F.nota('inicio', 'Ir para Início') > 0);
    assert.equal(F.nota('xyz', 'Configurações'), -1);
    assert.equal(F.nota('', 'qualquer'), 0);
});

test('ordena: prefixo/início de palavra vence espalhado', () => {
    const r = F.filtrar(CMDS, 'conv');
    assert.deepEqual(r.slice(0, 2), ['Nova conversa', 'Exportar conversa'].sort((a, b) => r.indexOf(a) - r.indexOf(b)));
    assert.equal(F.filtrar(CMDS, 'mem')[0], 'Ir para Memória');
    assert.equal(F.filtrar(CMDS, 'tema')[0], 'Mudar tema');
});

test('consulta vazia devolve tudo na ordem original; sem casamento devolve vazio', () => {
    assert.deepEqual(F.filtrar(CMDS, '  '), CMDS);
    assert.deepEqual(F.filtrar(CMDS, 'zzzz'), []);
});

test('marcas: posições dos caracteres casados (trecho contíguo e espalhado)', () => {
    assert.deepEqual(F.marcas('mem', 'Ir para Memória'), [8, 9, 10]);
    assert.deepEqual(F.marcas('ipm', 'Ir para Memória'), [0, 3, 8]);
    assert.deepEqual(F.marcas('zz', 'abc'), []);
});

test('pega objetos por uma função de texto', () => {
    const itens = [{ t: 'alpha' }, { t: 'beta' }];
    assert.deepEqual(F.filtrar(itens, 'bet', x => x.t), [{ t: 'beta' }]);
});

test('contem: todas as palavras como trecho, sem acento, em qualquer ordem', () => {
    assert.equal(F.contem('backup', 'Backup diário do vault no iCloud').ok, true);
    assert.equal(F.contem('memoria sqlite', 'Memória nova em SQLite com busca híbrida').ok, true);
    assert.equal(F.contem('sqlite memoria', 'Memória nova em SQLite com busca híbrida').ok, true);
    // subsequência espalhada NÃO casa (é o que `nota` aceitaria)
    assert.equal(F.contem('backup', 'Bot do Telegram com botões de aprovar').ok, false);
    assert.equal(F.contem('xyz', 'Backup diário').ok, false);
    assert.deepEqual(F.contem('', 'qualquer').marcas, []);
});

test('contem: marcas cobrem exatamente os trechos achados', () => {
    const r = F.contem('dia', 'Backup diário');
    assert.deepEqual(r.marcas, [7, 8, 9]);   // "dia" dentro de "diário"
});

test('buscar: ordena por trecho no início da palavra e descarta o resto', () => {
    const itens = ['Resumo do backup', 'Backup diário', 'Telegram bot', 'Outro assunto'];
    assert.deepEqual(F.buscar(itens, 'backup'), ['Backup diário', 'Resumo do backup']);
    assert.deepEqual(F.buscar(itens, '  '), itens);
});

test('ocorrencias: todas, sem acento, sem diferenciar maiúsculas, sem sobrepor', () => {
    assert.deepEqual(F.ocorrencias('Fase 0 e fase 1; FASE', 'fase'), [[0, 4], [9, 13], [17, 21]]);
    assert.deepEqual(F.ocorrencias('Memória nova', 'memoria'), [[0, 7]]);
    assert.deepEqual(F.ocorrencias('aaaa', 'aa'), [[0, 2], [2, 4]]);
    assert.deepEqual(F.ocorrencias('nada aqui', 'xyz'), []);
    assert.deepEqual(F.ocorrencias('qualquer', '  '), []);
    assert.deepEqual(F.ocorrencias('qualquer', ''), []);
});
