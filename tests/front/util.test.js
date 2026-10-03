const test = require('node:test');
const assert = require('node:assert/strict');
const U = require('../../Orion_Core/Front_end_Orion/js/util.js');

const AGORA = new Date(2026, 9, 3, 15, 30);          // sáb 03/10/2026 15:30 (local)
const dia = (n, h = 12) => new Date(2026, 9, 3 - n, h, 0);

test('norm: sem acento e minúsculo', () => {
    assert.equal(U.norm('Memória Çafé ÀÉÎ'), 'memoria cafe aei');
});

test('rotuloDia e agruparPorDia: hoje, ontem, semana, mês, meses antigos', () => {
    assert.equal(U.rotuloDia(dia(0), AGORA), 'Hoje');
    assert.equal(U.rotuloDia(new Date(2026, 9, 3, 0, 5), AGORA), 'Hoje');
    assert.equal(U.rotuloDia(dia(1, 23), AGORA), 'Ontem');
    assert.equal(U.rotuloDia(dia(4), AGORA), 'Esta semana');
    assert.equal(U.rotuloDia(dia(12), AGORA), 'Este mês');
    assert.equal(U.rotuloDia(new Date(2026, 5, 10), AGORA), 'Junho');
    assert.equal(U.rotuloDia(new Date(2025, 11, 1), AGORA), 'Dezembro de 2025');
    assert.equal(U.rotuloDia('', AGORA), 'Sem data');
    assert.equal(U.rotuloDia('lixo', AGORA), 'Sem data');
    const g = U.agruparPorDia([{ d: dia(0) }, { d: dia(0, 8) }, { d: dia(1) }, { d: dia(6) }, { d: dia(9) }], x => x.d, AGORA);
    assert.deepEqual(g.map(x => [x.rotulo, x.itens.length]), [['Hoje', 2], ['Ontem', 1], ['Esta semana', 1], ['Este mês', 1]]);
});

test('quando: agora, minutos, horas, ontem, data', () => {
    assert.equal(U.quando(new Date(AGORA - 10_000), AGORA), 'agora');
    assert.equal(U.quando(new Date(AGORA - 5 * 60_000), AGORA), 'há 5 min');
    assert.equal(U.quando(new Date(AGORA - 3 * 3600_000), AGORA), 'há 3 h');
    assert.equal(U.quando(dia(1), AGORA), 'ontem');
    assert.equal(U.quando(dia(5), AGORA), '28/09');
    assert.equal(U.quando(null, AGORA), '');
});

test('hora, dataCurta e saudacao', () => {
    assert.equal(U.hora(new Date(2026, 0, 1, 7, 5)), '07:05');
    assert.equal(U.hora('x'), '');
    assert.equal(U.dataCurta(new Date(2026, 0, 9)), '09/01');
    const s = h => U.saudacao(new Date(2026, 0, 1, h));
    assert.deepEqual([s(3), s(9), s(14), s(20)], ['Boa madrugada', 'Bom dia', 'Boa tarde', 'Boa noite']);
});

test('formatação numérica tolera nulos e valores ruins', () => {
    assert.equal(U.fmtMs(1840), '1840 ms');
    assert.equal(U.fmtMs(12500), '12.5 s');
    assert.equal(U.fmtMs(null), '—');
    assert.equal(U.fmtPct(63.6), '64%');
    assert.equal(U.fmtPct('x'), '—');
    assert.equal(U.fmtNum(3091204), '3.091.204');
    assert.equal(U.fmtBytes(1536), '1.5 KB');
    assert.equal(U.fmtBytes(5 * 1024 * 1024), '5.0 MB');
    assert.equal(U.fmtBytes(NaN), '—');
});

test('hostDe, truncar, clamp', () => {
    assert.equal(U.hostDe('https://www.sqlite.org/fts5.html?a=1&amp;b=2'), 'sqlite.org');
    assert.equal(U.hostDe('nao-e-url'), '');
    assert.equal(U.truncar('abcdef', 4), 'abc…');
    assert.equal(U.truncar('abc', 4), 'abc');
    assert.equal(U.clamp(5, 0, 3), 3);
});

test('noProximoQuadro agrupa várias chamadas em uma execução', () => {
    const fila = [];
    let n = 0;
    const f = U.noProximoQuadro(() => n++, cb => fila.push(cb));
    f(); f(); f();
    assert.equal(fila.length, 1);
    fila.shift()();
    assert.equal(n, 1);
    f();
    assert.equal(fila.length, 1);   // pode agendar de novo depois de executar
});

test('debounce adia e cancela', async () => {
    let n = 0;
    const d = U.debounce(() => n++, 20);
    d(); d(); d();
    await new Promise(r => setTimeout(r, 50));
    assert.equal(n, 1);
    d(); d.cancel();
    await new Promise(r => setTimeout(r, 40));
    assert.equal(n, 1);
});

test('uid é único', () => {
    assert.equal(new Set(Array.from({ length: 500 }, () => U.uid())).size, 500);
});

test('fmtDur: ms, segundos com vírgula e minutos', () => {
    assert.equal(U.fmtDur(0), '0 ms');
    assert.equal(U.fmtDur(840), '840 ms');
    assert.equal(U.fmtDur(2140), '2,1 s');
    assert.equal(U.fmtDur(59900), '59,9 s');
    assert.equal(U.fmtDur(72000), '1 min 12 s');
    assert.equal(U.fmtDur(125000), '2 min 05 s');
    assert.equal(U.fmtDur(-1), '');
    assert.equal(U.fmtDur(NaN), '');
});
