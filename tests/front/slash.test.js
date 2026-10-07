const test = require('node:test');
const assert = require('node:assert/strict');
const S = require('../../Orion_Core/Front_end_Orion/js/slash.js');

test('"/" sozinho lista todos os comandos; prefixo filtra', () => {
    assert.equal(S.sugerir('/').length, S.COMANDOS.length);
    assert.deepEqual(S.sugerir('/mo').map(s => s.cmd), ['modelo']);
    assert.deepEqual(S.sugerir('/m').map(s => s.cmd), ['modelo', 'mudo', 'memoria']);
    assert.deepEqual(S.sugerir('/ME').map(s => s.cmd), ['memoria']);        // sem diferenciar maiúsculas
    assert.deepEqual(S.sugerir('/zzz'), []);
});

test('comando que pede argumento completa com espaço; sem argumento já "fecha"', () => {
    const modelo = S.sugerir('/modelo')[0];
    assert.equal(modelo.completar, '/modelo ');
    assert.equal(modelo.fecha, false);
    const nova = S.sugerir('/nova')[0];
    assert.equal(nova.completar, '/nova');
    assert.equal(nova.fecha, true);
});

test('argumentos fixos são sugeridos por prefixo depois do espaço', () => {
    assert.deepEqual(S.sugerir('/tema ').map(s => s.arg), ['noite', 'grafite', 'contraste']);
    assert.deepEqual(S.sugerir('/tema gr').map(s => s.completar), ['/tema grafite']);
    assert.equal(S.sugerir('/tema gr')[0].fecha, true);
    assert.deepEqual(S.sugerir('/tema xx'), []);
    assert.deepEqual(S.sugerir('/nova '), []);                               // sem argumento a oferecer
});

test('texto com várias linhas ou sem barra nunca é sugestão', () => {
    assert.deepEqual(S.sugerir('oi /nova'), []);
    assert.deepEqual(S.sugerir('/nova\nmais uma linha'), []);
    assert.deepEqual(S.sugerir(''), []);
    assert.deepEqual(S.sugerir(null), []);
});

test('interpretar: comando válido, com argumento, inválido, desconhecido e texto comum', () => {
    assert.deepEqual(S.interpretar('/nova'), { cmd: 'nova' });
    assert.deepEqual(S.interpretar('  /nova  '), { cmd: 'nova' });
    assert.deepEqual(S.interpretar('/modelo GROQ'), { cmd: 'modelo', arg: 'groq' });
    assert.deepEqual(S.interpretar('/buscar a fase 0'), { cmd: 'buscar', arg: 'a fase 0' });
    assert.equal(S.interpretar('/modelo gpt').invalido, true);
    assert.equal(S.interpretar('/modelo').invalido, true);
    assert.equal(S.interpretar('/buscar').invalido, true);
    assert.equal(S.interpretar('/xyz').desconhecido, true);
    assert.equal(S.interpretar('oi, tudo bem?'), null);
});

test('caminho de arquivo e texto de várias linhas NÃO são comando (vão ao modelo)', () => {
    assert.equal(S.interpretar('/home/antonio/notas.txt'), null);
    assert.equal(S.interpretar('/etc/hosts é seguro?'), null);
    assert.equal(S.interpretar('/nova\ne depois explique'), null);
    assert.equal(S.interpretar('C:\\Users\\x'), null);
});

test('interpretar aceita acento digitado: /memória e /integrações', () => {
    assert.deepEqual(S.interpretar('/memória'), { cmd: 'memoria' });
    assert.deepEqual(S.interpretar('/integrações'), { cmd: 'integracoes' });
});

test('todo comando tem nome único em minúsculas e descrição', () => {
    const nomes = S.COMANDOS.map(c => c.nome);
    assert.equal(new Set(nomes).size, nomes.length);
    for (const c of S.COMANDOS) { assert.match(c.nome, /^[a-z]+$/); assert.ok(c.desc.length > 3); }
});

test('skills usam namespace sem colidir com comandos, caminhos ou barra literal', () => {
    assert.deepEqual(S.interpretar('/pesquisa:revisar orçamento'), { skill: 'pesquisa:revisar', arg: 'orçamento' });
    assert.deepEqual(S.interpretar('/modelo:revisar x\ny'), { skill: 'modelo:revisar', arg: 'x\ny' });
    assert.deepEqual(S.interpretar('/nova'), { cmd: 'nova' });
    assert.deepEqual(S.interpretar('/modelo groq'), { cmd: 'modelo', arg: 'groq' });
    assert.equal(S.interpretar('//pesquisa:revisar x'), null);
    assert.equal(S.interpretar('/home/usuario/arquivo.md'), null);
    const skills = [{ id: 'pesquisa:revisar', description: 'Revisar', enabled: true },
        { id: 'pesquisa:oculta', description: 'Desativada', enabled: false }];
    assert.equal(S.sugerir('/pes', skills).length, 1);
    assert.equal(S.sugerir('/pes', skills)[0].completar, '/pesquisa:revisar ');
    assert.equal(S.sugerir('/pesquisa:revisar pedido', skills).length, 0);
});
