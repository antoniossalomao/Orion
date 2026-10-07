const test = require('node:test');
const assert = require('node:assert/strict');
const P = require('../../Orion_Core/Front_end_Orion/js/painel.js');

test('duracao: segundos, minutos, horas e dias; tolera lixo', () => {
    assert.equal(P.duracao(45), '45 s');
    assert.equal(P.duracao(150), '3 min');
    assert.equal(P.duracao(7200), '2 h');
    assert.equal(P.duracao(259200), '3 d');
    assert.equal(P.duracao(-5), '0 s');
    assert.equal(P.duracao('x'), '0 s');
    assert.equal(P.duracao(null), '0 s');
});

test('estadoModelo: quarentena > só falhas > instável > sem uso > ok', () => {
    assert.deepEqual(P.estadoModelo({ quarentena_s: 140, chamadas: 5, ok: 5 }),
        { estado: 'danger', rotulo: 'Em quarentena · volta em 2 min' });
    assert.deepEqual(P.estadoModelo({ chamadas: 3, ok: 0, falhas: 3 }), { estado: 'danger', rotulo: 'Só falhas' });
    assert.equal(P.estadoModelo({ chamadas: 10, ok: 6, falhas: 4 }).estado, 'warn');     // 40% de falha
    assert.equal(P.estadoModelo({ chamadas: 10, ok: 9, falhas: 1 }).estado, 'ok');       // 10%: aceitável
    assert.equal(P.estadoModelo({ chamadas: 0, ok: 0, falhas: 0, pulos: 0 }).estado, 'idle');
    assert.equal(P.estadoModelo({}).estado, 'idle');
    assert.equal(P.estadoModelo(null).estado, 'idle');
    assert.equal(P.estadoModelo({ chamadas: 0, pulos: 2 }).estado, 'ok');                // foi pulado, não é "sem uso"
});

test('usoCli: porcentagem, severidade e texto; não instalada não mede', () => {
    assert.deepEqual(P.usoCli({ instalada: true, usadas_hoje: 4, limite_diario: 20 }),
        { pct: 20, sev: 'normal', texto: '16 restantes hoje', valor: '4/20' });
    assert.equal(P.usoCli({ instalada: true, usadas_hoje: 16, limite_diario: 20 }).sev, 'alto');
    const cheia = P.usoCli({ instalada: true, usadas_hoje: 20, limite_diario: 20 });
    assert.deepEqual([cheia.sev, cheia.texto, cheia.pct], ['critico', 'esgotada hoje', 100]);
    assert.equal(P.usoCli({ instalada: true, usadas_hoje: 99, limite_diario: 20 }).pct, 100);  // teto
    assert.deepEqual(P.usoCli({ instalada: false, usadas_hoje: 0, limite_diario: 20 }),
        { pct: 0, sev: 'nd', texto: 'não instalada', valor: '—' });
    assert.equal(P.usoCli({ instalada: true, usadas_hoje: 1, limite_diario: 0 }).pct, 100);    // limite 0 não divide por zero
    assert.equal(P.usoCli(null).sev, 'nd');
});

test('alertas: o grave vem primeiro e cada problema aparece com texto', () => {
    const p = {
        memoria: { ok: true },
        modelos: { configurado: true, endpoints: [
            { nome: 'a', chamadas: 5, ok: 5, falhas: 0, quarentena_s: 0 },
            { nome: 'b', chamadas: 4, ok: 0, falhas: 4, quarentena_s: 90 },
            { nome: 'c', chamadas: 10, ok: 6, falhas: 4 }] },
        clis: [{ nome: 'gemini', instalada: true, usadas_hoje: 20, limite_diario: 20 },
               { nome: 'codex', instalada: false, usadas_hoje: 0, limite_diario: 20 }],
        aprovacoes: { pendentes: 2 },
        jobs: { erros: ['backup: disco cheio'] },
        mcp: { google: 'ok (12 ferramentas)', web: 'falhou: TimeoutError' },
    };
    const a = P.alertas(p);
    assert.equal(a[0].nivel, 'danger');
    assert.match(a[0].texto, /Modelo “b”: em quarentena/);
    const textos = a.map(x => x.texto).join('\n');
    assert.match(textos, /Modelo “c” está instável \(4 falha\(s\) de 10\)/);
    assert.match(textos, /CLI “gemini” esgotou/);
    assert.doesNotMatch(textos, /codex/);                       // não instalada não é problema
    assert.match(textos, /2 ações esperam o seu aval/);
    assert.match(textos, /Job com erro: backup: disco cheio/);
    assert.match(textos, /MCP “web”: falhou: TimeoutError/);
    assert.doesNotMatch(textos, /google/);
    assert.equal(a.findIndex(x => x.nivel === 'warn') > a.findLastIndex(x => x.nivel === 'danger'), true);
});

test('alertas: tudo bem não gera nada; gateway ausente e memória fora geram', () => {
    assert.deepEqual(P.alertas({ memoria: { ok: true }, modelos: { configurado: true, endpoints: [] }, clis: [],
        aprovacoes: { pendentes: 0 }, jobs: { erros: [] }, mcp: {} }), []);
    assert.deepEqual(P.alertas(null), []);
    assert.deepEqual(P.alertas({}), []);
    const t = P.alertas({ memoria: { ok: false }, modelos: { configurado: false } });
    assert.equal(t[0].nivel, 'danger');
    assert.match(t[0].texto, /memória/);
    assert.match(t[1].texto, /ORION_GATEWAY_URL/);
    assert.equal(P.alertas({ aprovacoes: { pendentes: 1 } })[0].texto, '1 ação espera o seu aval.');
});

test('resumoDecisoes e rótulos de ação', () => {
    assert.equal(P.resumoDecisoes({ janela_h: 24, total: 0 }), 'Nenhuma decisão nas últimas 24 h.');
    assert.equal(P.resumoDecisoes({ total: 1, por_acao: { allow: 1, confirm: 0, deny: 0 } }),
        '1 decisão: 1 liberada, 0 pediram aval, 0 negadas.');
    assert.equal(P.resumoDecisoes({ total: 12, por_acao: { allow: 9, confirm: 2, deny: 1 } }),
        '12 decisões: 9 liberadas, 2 pediram aval, 1 negada.');
    assert.equal(P.resumoDecisoes(null), 'Nenhuma decisão nas últimas 24 h.');
    assert.deepEqual(['allow', 'confirm', 'deny', 'x', null].map(P.rotuloAcao), ['Liberada', 'Pediu aval', 'Negada', 'x', '—']);
    assert.deepEqual(['allow', 'confirm', 'deny', 'x'].map(P.tomAcao), ['ok', 'warn', 'danger', 'muted']);
});

test('resumoRoteamento: só aparece com o roteamento ligado', () => {
    assert.equal(P.resumoRoteamento({ ativo: true, contagem: { rapido: 12, pesado: 3, visao: 1 } }),
        '12 rápidas · 3 pesadas · 1 com imagem');
    assert.equal(P.resumoRoteamento({ ativo: true }), '0 rápidas · 0 pesadas · 0 com imagem');
    assert.equal(P.resumoRoteamento({ ativo: false, contagem: { rapido: 9 } }), null);
    assert.equal(P.resumoRoteamento(undefined), null);
});

test('resumoProvedores: do que mais serviu para o que menos serviu', () => {
    assert.equal(P.resumoProvedores({ groq: 8, gemini: 30, vazio: 0 }), 'gemini ×30 · groq ×8');
    assert.equal(P.resumoProvedores({}), '');
    assert.equal(P.resumoProvedores(null), '');
    assert.equal(P.resumoProvedores({ x: 'lixo' }), '');
});

test('resumoVoz: desligada sem opt-in ou chave; ligada mostra o uso', () => {
    assert.deepEqual(P.resumoVoz(null), { clique: 'desligada', aoVivo: 'desligada' });
    assert.deepEqual(P.resumoVoz({ clique: { ligada: false }, ao_vivo: { ligada: false } }),
        { clique: 'desligada', aoVivo: 'desligada' });
    assert.deepEqual(P.resumoVoz({ clique: { ligada: true, fala: true, turnos: 7 }, ao_vivo: { ligada: true, sessoes: 2, ativas: 1, minutos: 12.5 } }),
        { clique: '7 fala(s), com resposta falada', aoVivo: '2 sessão(ões), 12.5 min · uma aberta agora' });
    assert.equal(P.resumoVoz({ clique: { ligada: true, fala: false, turnos: 0 } }).clique, '0 fala(s), só texto');
});

test('alertas: falha de voz só avisa se a voz está ligada', () => {
    const base = { voz: { falhas: 2, ultimo_erro: 'a fala falhou: Timeout', clique: { ligada: true }, ao_vivo: { ligada: false } } };
    assert.ok(P.alertas(base).some(a => a.texto.includes('Voz: 2 falha(s) (última: a fala falhou: Timeout)')));
    assert.ok(!P.alertas({ voz: { ...base.voz, clique: { ligada: false } } }).some(a => a.texto.startsWith('Voz')));
});
