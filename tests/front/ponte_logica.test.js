const test = require('node:test');
const assert = require('node:assert/strict');
const P = require('../../Orion_Core/Front_end_Orion/js/ponte_logica.js');

test('token do fragmento: só caracteres seguros e tamanho razoável', () => {
    assert.equal(P.tokenDoFragmento('#t=abcdefghijklmnop_-1234'), 'abcdefghijklmnop_-1234');
    assert.equal(P.tokenDoFragmento('#x=1&t=abcdefghijklmnop'), 'abcdefghijklmnop');
    assert.equal(P.tokenDoFragmento('#t=curto'), '');
    assert.equal(P.tokenDoFragmento('#t=abc def ghi jkl mno pqr'), '');
    assert.equal(P.tokenDoFragmento('#t=<script>alert(1)</script>'), '');
    assert.equal(P.tokenDoFragmento(''), '');
    assert.equal(P.tokenDoFragmento(undefined), '');
});

test('modo da busca: só os conhecidos', () => {
    assert.equal(P.modoDaBusca('?modo=isso'), 'isso');
    assert.equal(P.modoDaBusca('?modo=rapido'), 'rapido');
    assert.equal(P.modoDaBusca('?modo=hack'), 'rapido');
    assert.equal(P.modoDaBusca(''), 'rapido');
});

test('resumo da captura mostra tipo, data e valor', () => {
    assert.equal(P.resumoDaCaptura({ tipo: 'tarefa', titulo: 'pagar boleto' }), 'Tarefa: pagar boleto');
    assert.equal(P.resumoDaCaptura({ tipo: 'lembrete', titulo: 'dentista', quando: '2026-10-11T15:30' }), 'Lembrete: dentista · para 11/10 às 15:30');
    assert.equal(P.resumoDaCaptura({ tipo: 'gasto', titulo: 'almoço', valor: 42.5 }), 'Gasto: almoço · R$ 42,50');
    assert.equal(P.resumoDaCaptura(null), 'Guardado.');
    assert.equal(P.formatarQuando('lixo'), 'lixo');
});

test('erros da captura têm mensagem própria', () => {
    assert.match(P.erroDaCaptura(403), /parear/);
    assert.equal(P.erroDaCaptura(409, 'defina ORION_VAULT_DIR'), 'defina ORION_VAULT_DIR');
    assert.match(P.erroDaCaptura(422), /2000/);
    assert.match(P.erroDaCaptura(0), /sem conexão/);
});

test('imagem da busca: só o id no formato que o servidor gera', () => {
    assert.equal(P.imagemDaBusca('?modo=isso&imagem=AbCd1234_-xyz'), 'AbCd1234_-xyz');
    assert.equal(P.imagemDaBusca('?imagem=curto'), '');
    assert.equal(P.imagemDaBusca('?imagem=../../etc/passwd'), '');
    assert.equal(P.imagemDaBusca(''), '');
});

test('erros do "o que é isso?" e o aviso da primeira vez', () => {
    assert.match(P.erroDaExplicacao(404), /expirou/);
    assert.equal(P.erroDaExplicacao(409, 'ligue ORION_VISION_TOOLS'), 'ligue ORION_VISION_TOOLS');
    assert.match(P.erroDaExplicacao(502, 'Timeout'), /Timeout/);
    assert.match(P.erroDaExplicacao(403), /parear/);
    assert.match(P.AVISO_PRIMEIRA_VEZ, /sair do computador/);
});
