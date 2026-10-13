const test = require('node:test');
const assert = require('node:assert/strict');
const L = require('../../Orion_Core/Front_end_Orion/js/automacao_logica.js');

test('estado da rotina: liberada, mudou, sem liberação, erro e desconhecido', () => {
    assert.deepEqual(L.estadoDaRotina('concedida'), { rotulo: 'Liberada', tom: 'ok' });
    assert.equal(L.estadoDaRotina('mudou').tom, 'aviso');
    assert.equal(L.estadoDaRotina('sem_concessao').tom, 'neutro');
    assert.equal(L.estadoDaRotina('erro').tom, 'erro');
    assert.deepEqual(L.estadoDaRotina('x'), { rotulo: 'x', tom: 'neutro' });
});

test('gatilhos legíveis', () => {
    assert.equal(L.gatilhosLegiveis([]), 'só manual');
    assert.equal(L.gatilhosLegiveis(undefined), 'só manual');
    assert.equal(L.gatilhosLegiveis([{ tipo: 'frase', valor: 'bom dia' }, { tipo: 'palmas' }, { tipo: 'tecla', valor: 'ctrl+alt+b' }, { tipo: 'telegram' }]),
        'frase “bom dia”, palmas, tecla ctrl+alt+b, Telegram');
});

test('resumo de execução da rotina', () => {
    assert.match(L.resumoDaExecucao({ nome: 'bom-dia', status: 'concluida', passos: [1, 2, 3] }), /concluída \(3 passo/);
    assert.match(L.resumoDaExecucao({ nome: 'x', status: 'aguardando_aprovacao' }), /espera a sua aprovação/);
    assert.match(L.resumoDaExecucao({ nome: 'x', status: 'nao_concedida' }), /não foi liberada/);
    assert.match(L.resumoDaExecucao({ nome: 'x', status: 'recusada', motivo: 'pânico' }), /recusada: pânico/);
    assert.match(L.resumoDaExecucao({ nome: 'x', status: 'falhou', motivo: 'passo 2: tempo esgotado' }), /falhou: passo 2/);
    assert.equal(L.resumoDaExecucao(null), '');
});

test('tarefa em segundo plano: status, uma por vez, limites e propostas abertas', () => {
    assert.equal(L.statusDaTarefa('rodando'), 'Rodando');
    assert.equal(L.statusDaTarefa('limite'), 'Parou no limite');
    assert.equal(L.podeCriarTarefa([{ status: 'concluida' }]), true);
    assert.equal(L.podeCriarTarefa([{ status: 'rodando' }]), false);
    assert.deepEqual(L.limitesDaTarefa('', 'abc'), { max_passos: 10, max_min: 10 });
    assert.deepEqual(L.limitesDaTarefa('99', '0'), { max_passos: 30, max_min: 10 });
    assert.deepEqual(L.limitesDaTarefa('5', '2.9'), { max_passos: 5, max_min: 2 });
    const t = { proposals: [{ texto: 'a', estado: 'aceita' }, { texto: 'b', estado: 'pendente' }, { texto: 'c', estado: 'pendente' }] };
    assert.deepEqual(L.propostasAbertas(t).map(p => [p.indice, p.texto]), [[1, 'b'], [2, 'c']]);
    assert.deepEqual(L.propostasAbertas(null), []);
});

test('Downloads e "posso desligar?"', () => {
    assert.match(L.resumoDeDownloads({ status: 'sem_regras' }), /regras_downloads\.yaml/);
    assert.match(L.resumoDeDownloads({ status: 'nao_concedida' }), /não foram liberadas/);
    assert.equal(L.resumoDeDownloads({ status: 'ok', movidos: [], ficaram: 0 }), 'nada a mover.');
    assert.equal(L.resumoDeDownloads({ status: 'ok', movidos: [1, 2], ficaram: 3, simulado: true, recusados: ['x'] }), 'moveria 2 arquivo(s) · 3 ficaram · 1 recusado(s).');
    assert.equal(L.resumoDeDesligar({ pode: true, itens: [] }), 'Pode desligar: nada pendente.');
    assert.equal(L.resumoDeDesligar({ pode: false, itens: [{ ok: true, texto: 'a' }, { ok: false, texto: 'repo: 2 arquivo(s)' }, { ok: false, texto: 'backup pendente' }] }), 'Ainda não: repo: 2 arquivo(s) · backup pendente');
});
