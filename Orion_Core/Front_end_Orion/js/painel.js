/* ==========================================================================
   ORION — painel.js | lógica pura do painel único (sem DOM): estados, alertas, formatação
   UMD: no navegador publica `Orion.painelLogica`; em Node, `require('./painel.js')`.
   Entrada: o JSON de `GET /painel` (ver orion/painel.py). Tudo tolera campo ausente.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).painelLogica = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    const num = v => (Number.isFinite(+v) ? +v : 0);

    /** 45 → "45 s", 150 → "3 min", 7200 → "2 h", 259200 → "3 d" */
    function duracao(seg) {
        const s = Math.max(0, Math.round(num(seg)));
        if (s < 90) return `${s} s`;
        if (s < 5400) return `${Math.round(s / 60)} min`;
        if (s < 172800) return `${Math.round(s / 3600)} h`;
        return `${Math.round(s / 86400)} d`;
    }

    /**
     * Estado de um endpoint de modelo, com texto (não só cor): quarentena por cota é o que mais
     * importa ao usuário, depois só-falhas, depois instável.
     * @returns {{estado: 'ok'|'warn'|'danger'|'idle', rotulo: string}}
     */
    function estadoModelo(e) {
        const q = num(e?.quarentena_s), chamadas = num(e?.chamadas), ok = num(e?.ok), falhas = num(e?.falhas);
        if (q > 0) return { estado: 'danger', rotulo: `${e?.motivo === 'falhas' ? 'Em pausa por falhas' : 'Em quarentena'} · volta em ${duracao(q)}` };
        if (chamadas > 0 && ok === 0 && falhas > 0) return { estado: 'danger', rotulo: 'Só falhas' };
        if (falhas > 0 && falhas / Math.max(1, ok + falhas) > 0.3) return { estado: 'warn', rotulo: 'Instável' };
        if (chamadas === 0 && num(e?.pulos) === 0) return { estado: 'idle', rotulo: 'Sem uso ainda' };
        return { estado: 'ok', rotulo: 'Funcionando' };
    }

    /**
     * Uso diário de uma CLI oficial. `sev` casa com `.meter[data-sev]` do CSS.
     * @returns {{pct: number, sev: 'normal'|'alto'|'critico'|'nd', texto: string, valor: string}}
     */
    function usoCli(c) {
        if (!c?.instalada) return { pct: 0, sev: 'nd', texto: 'não instalada', valor: '—' };
        const limite = Math.max(1, num(c.limite_diario)), usadas = num(c.usadas_hoje);
        const pct = Math.min(100, Math.round((usadas / limite) * 100));
        const sev = pct >= 100 ? 'critico' : pct >= 80 ? 'alto' : 'normal';
        const texto = pct >= 100 ? 'esgotada hoje' : `${Math.max(0, limite - usadas)} restantes hoje`;
        return { pct, sev, texto, valor: `${usadas}/${limite}` };
    }

    /**
     * O que merece atenção agora, do mais grave para o menos. `nivel`: 'danger' | 'warn'.
     * Só descreve o que o painel já mostra; nunca inventa causa.
     */
    function alertas(p) {
        const a = [];
        const add = (nivel, texto) => a.push({ nivel, texto });
        if (!p) return a;
        if (p.modos?.panico) add('danger', 'Modo pânico ligado: rede, execução, memória da tela e escuta estão cortadas até você sair.');
        if (p.memoria && p.memoria.ok === false) add('danger', 'A memória (SQLite) não respondeu.');
        if (p.modelos && p.modelos.configurado === false) {
            add('warn', 'Nenhum modelo configurado: o chat está desligado (ORION_GATEWAY_URL e ORION_GATEWAY_MODEL).');
        }
        for (const e of p.modelos?.endpoints || []) {
            const s = estadoModelo(e);
            if (s.estado === 'danger') add('danger', `Modelo “${e.nome}”: ${s.rotulo.toLowerCase()}.`);
            else if (s.estado === 'warn') add('warn', `Modelo “${e.nome}” está instável (${num(e.falhas)} falha(s) de ${num(e.chamadas)}).`);
        }
        for (const c of p.clis || []) {
            if (c.instalada && usoCli(c).sev === 'critico') add('warn', `A CLI “${c.nome}” esgotou o limite de hoje.`);
        }
        for (const c of p.cota || []) {
            const u = usoCota(c);
            if (u.sev === 'critico') add('warn', `Cota gratuita de ${c.nome} passou do limite (${u.valor}): jobs opcionais parados; o chat continua.`);
            else if (u.sev === 'alto') add('warn', `Cota gratuita de ${c.nome} em ${u.pct}%: jobs opcionais parados até virar o dia.`);
        }
        const pend = num(p.aprovacoes?.pendentes);
        if (pend > 0) add('warn', pend === 1 ? '1 ação espera o seu aval.' : `${pend} ações esperam o seu aval.`);
        for (const e of p.jobs?.erros || []) add('warn', `Job com erro: ${e}`);
        for (const [nome, st] of Object.entries(p.mcp || {})) {
            if (!String(st).startsWith('ok')) add('warn', `Servidor MCP “${nome}”: ${st}`);
        }
        if (p.voz && num(p.voz.falhas) > 0 && (p.voz.clique?.ligada || p.voz.ao_vivo?.ligada)) {
            add('warn', `Voz: ${num(p.voz.falhas)} falha(s)${p.voz.ultimo_erro ? ` (última: ${p.voz.ultimo_erro})` : ''}.`);
        }
        if (p.voz?.escuta?.pedida && !p.voz.escuta.ouvindo) {
            add('warn', `Palavra de ativação: não está ouvindo${p.voz.escuta.ultimo_erro ? ` (${p.voz.escuta.ultimo_erro})` : ''}.`);
        }
        return a.sort((x, y) => (x.nivel === y.nivel ? 0 : x.nivel === 'danger' ? -1 : 1));
    }

    /** Texto curto da política nas últimas horas: "12 decisões: 9 liberadas, 2 pediram aval, 1 negada". */
    function resumoDecisoes(d) {
        const total = num(d?.total), ac = d?.por_acao || {};
        const plural = (n, um, varios) => `${n} ${n === 1 ? um : varios}`;
        if (!total) return `Nenhuma decisão nas últimas ${num(d?.janela_h) || 24} h.`;
        return `${plural(total, 'decisão', 'decisões')}: ${plural(num(ac.allow), 'liberada', 'liberadas')}, `
            + `${plural(num(ac.confirm), 'pediu aval', 'pediram aval')}, ${plural(num(ac.deny), 'negada', 'negadas')}.`;
    }

    /** "12 rápidas · 3 pesadas · 1 com imagem" (ou null se o roteamento está desligado) */
    function resumoRoteamento(r) {
        if (!r?.ativo) return null;
        const c = r.contagem || {};
        return `${num(c.rapido)} rápidas · ${num(c.pesado)} pesadas · ${num(c.visao)} com imagem`;
    }

    /** Voz no painel: {clique, ao_vivo} em texto; "desligada" quando falta opt-in, chave ou gateway. */
    function resumoVoz(v) {
        const c = v?.clique, l = v?.ao_vivo, e = v?.escuta;
        const clique = !c?.ligada ? 'desligada'
            : `${num(c.turnos)} fala(s)${c.fala ? ', com resposta falada' : ', só texto'}`;
        const aoVivo = !l?.ligada ? 'desligada'
            : `${num(l.sessoes)} sessão(ões), ${num(l.minutos)} min${num(l.ativas) ? ' · uma aberta agora' : ''}`;
        const escuta = !e?.pedida ? 'desligada'
            : !e.ouvindo ? `não subiu${e.ultimo_erro ? `: ${e.ultimo_erro}` : ''}`
            : `${e.pausada ? 'pausada' : 'ouvindo'} · ${num(e.ativacoes)} ativação(ões)`;
        return { clique, aoVivo, escuta };
    }

    /** Duas palmas (regra 51): estado e a força das últimas, para calibrar ORION_CLAP_RATIO. */
    function resumoPalmas(v) {
        const p = v?.palmas;
        if (!p?.pedida) return 'desligadas';
        if (!p.ouvindo) return `não subiram${v?.escuta?.ultimo_erro ? `: ${v.escuta.ultimo_erro}` : ''}`;
        const picos = (p.picos_ultima_hora || []).slice(-6).map(x => `${String(Math.round(num(x) * 10) / 10).replace('.', ',')}×`);
        const partes = [`${v.escuta?.pausada ? 'pausadas' : 'ouvindo'} (${p.acao || 'abrir'})`, `${num(p.acionadas)} acionada(s)`];
        if (num(p.recusadas)) partes.push(`${num(p.recusadas)} recusada(s)`);
        partes.push(picos.length ? `força das últimas: ${picos.join(' ')}` : 'nenhuma palma na última hora');
        return partes.join(' · ');
    }

    /** 512 → "512 B", 1536 → "1,5 KB", 3_500_000 → "3,3 MB" (vírgula decimal, como o resto da tela) */
    function bytes(n) {
        const b = Math.max(0, Math.round(num(n)));
        if (b < 1024) return `${b} B`;
        const [v, u] = b < 1024 * 1024 ? [b / 1024, 'KB'] : [b / 1024 / 1024, 'MB'];
        return `${(Math.round(v * 10) / 10).toString().replace('.', ',')} ${u}`;
    }

    /**
     * Cota gratuita contada pelo Orion (regra 46). `sev` casa com `.meter[data-sev]` do CSS.
     * @returns {{pct: number, sev: 'normal'|'alto'|'critico'|'nd', texto: string, valor: string}}
     */
    function usoCota(c) {
        const usado = num(c?.usado), limite = num(c?.limite);
        const quando = c?.periodo === 'mes' ? 'no mês' : 'hoje';
        if (!limite) return { pct: 0, sev: 'nd', texto: `sem limite · ${usado} ${quando}`, valor: String(usado) };
        const pct = Math.min(100, Math.round((usado / limite) * 100));
        const sev = usado >= limite ? 'critico' : pct >= 90 ? 'alto' : 'normal';
        const texto = usado >= limite ? `passou do limite ${quando}: jobs opcionais parados`
            : pct >= 90 ? `jobs opcionais parados (${pct}%)` : `${limite - usado} restantes ${quando}`;
        return { pct, sev, texto, valor: `${usado}/${limite}` };
    }

    /** Linhas da tabela "Provedores" (a semana pelo registro de saída), mais usadas primeiro. */
    function linhasProvedores(lista) {
        return (lista || []).filter(x => x && x.provider).sort((a, b) => num(b.chamadas) - num(a.chamadas)).map(x => ({
            provedor: String(x.provider),
            tipo: String(x.kind || ''),
            chamadas: num(x.chamadas),
            falhas: num(x.falhas),
            latencia: `p50 ${num(x.p50_ms)} ms · p95 ${num(x.p95_ms)} ms`,
            modelo: String(x.modelo || '—'),
            trafego: `↑ ${bytes(x.bytes_out)} · ↓ ${bytes(x.bytes_in)}`,
        }));
    }

    /** Pânico e não perturbe em texto (regra 48). `hora(ts)` formata o horário (injetado para testar). */
    function resumoModos(m, hora = ts => new Date(ts * 1000).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })) {
        const panico = m?.panico ? 'LIGADO: rede, execução, memória da tela e escuta cortadas' : 'desligado';
        const dnd = !m?.nao_perturbe ? 'desligado'
            : m.nao_perturbe_ate ? `até ${hora(m.nao_perturbe_ate)}: avisos não urgentes esperando`
            : `no horário ${m.nao_perturbe_horario || ''}: avisos não urgentes esperando`.replace('  ', ' ');
        return { panico, dnd };
    }

    const ROTULO_ACAO = { allow: 'Liberada', confirm: 'Pediu aval', deny: 'Negada' };
    const TOM_ACAO = { allow: 'ok', confirm: 'warn', deny: 'danger' };
    const rotuloAcao = a => ROTULO_ACAO[a] || String(a || '—');
    const tomAcao = a => TOM_ACAO[a] || 'muted';

    return { duracao, estadoModelo, usoCli, usoCota, bytes, linhasProvedores, resumoModos, alertas, resumoVoz, resumoPalmas, resumoDecisoes, resumoRoteamento, rotuloAcao, tomAcao };
});
