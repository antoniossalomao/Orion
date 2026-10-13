/* ==========================================================================
   ORION — automacao_logica.js | lógica pura de E9 (sem DOM): rotinas, tarefa em segundo plano,
   Downloads e "posso desligar?"
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).automacaoLogica = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    const ESTADO_ROTINA = {
        concedida: { rotulo: 'Liberada', tom: 'ok' },
        mudou: { rotulo: 'O arquivo mudou: libere de novo', tom: 'aviso' },
        sem_concessao: { rotulo: 'Esperando a sua liberação', tom: 'neutro' },
        erro: { rotulo: 'Com erro', tom: 'erro' },
    };
    /** como mostrar o estado de uma rotina (ou do arquivo de regras de Downloads) */
    const estadoDaRotina = e => ESTADO_ROTINA[e] || { rotulo: String(e || 'desconhecido'), tom: 'neutro' };

    const GATILHO = { frase: g => `frase “${g.valor}”`, palmas: () => 'palmas', tecla: g => `tecla${g.valor ? ' ' + g.valor : ''}`, telegram: () => 'Telegram' };
    /** "frase “bom dia”, palmas" */
    const gatilhosLegiveis = gs => (gs || []).map(g => (GATILHO[g.tipo] || (() => g.tipo))(g)).join(', ') || 'só manual';

    /** o que a rotina fez, em uma frase (resposta de POST /rotinas/{nome}/executar) */
    function resumoDaExecucao(r) {
        if (!r) return '';
        const passos = Array.isArray(r.passos) ? r.passos.length : 0;
        switch (r.status) {
            case 'concluida': return `Rotina “${r.nome}” concluída (${passos} passo(s)).`;
            case 'aguardando_aprovacao': return `Rotina “${r.nome}” parou: um passo espera a sua aprovação.`;
            case 'nao_concedida': return `Rotina “${r.nome}” ainda não foi liberada.`;
            case 'recusada': return `Rotina “${r.nome}” recusada${r.motivo ? `: ${r.motivo}` : '.'}`;
            default: return `Rotina “${r.nome}” falhou${r.motivo ? `: ${r.motivo}` : '.'}`;
        }
    }

    const STATUS_TAREFA = { rodando: 'Rodando', concluida: 'Concluída', falhou: 'Falhou', cancelada: 'Cancelada', limite: 'Parou no limite' };
    const statusDaTarefa = s => STATUS_TAREFA[s] || String(s || '');
    /** só uma tarefa em segundo plano roda por vez */
    const podeCriarTarefa = tarefas => !(tarefas || []).some(t => t.status === 'rodando');

    /** limites do formulário: passos 1–30 e minutos 1–30 */
    function limitesDaTarefa(passos, minutos) {
        const n = (v, d) => { const x = Math.trunc(Number(v)); return Number.isFinite(x) && x >= 1 ? Math.min(x, 30) : d; };
        return { max_passos: n(passos, 10), max_min: n(minutos, 10) };
    }

    /** propostas ainda abertas de uma tarefa */
    const propostasAbertas = t => (t?.proposals || []).map((p, indice) => ({ ...p, indice })).filter(p => p.estado === 'pendente');

    /** resumo do que o organizador de Downloads moveu (ou moveria, com `simulado`) */
    function resumoDeDownloads(r) {
        if (!r) return '';
        if (r.status === 'sem_regras') return 'Não há regras: crie regras_downloads.yaml na pasta de dados.';
        if (r.status === 'nao_concedida') return 'As regras ainda não foram liberadas (ou o arquivo mudou).';
        const n = (r.movidos || []).length, ficou = r.ficaram || 0;
        const verbo = r.simulado ? 'moveria' : 'moveu';
        const recusados = (r.recusados || []).length;
        return `${n ? `${verbo} ${n} arquivo(s)` : 'nada a mover'}${ficou ? ` · ${ficou} ficaram` : ''}${recusados ? ` · ${recusados} recusado(s)` : ''}.`;
    }

    /** "posso desligar?": só os itens que impedem, ou a confirmação */
    function resumoDeDesligar(r) {
        if (!r) return '';
        if (r.pode) return 'Pode desligar: nada pendente.';
        return `Ainda não: ${(r.itens || []).filter(i => !i.ok).map(i => i.texto).join(' · ')}`;
    }

    return { estadoDaRotina, gatilhosLegiveis, resumoDaExecucao, statusDaTarefa, podeCriarTarefa, limitesDaTarefa, propostasAbertas, resumoDeDownloads, resumoDeDesligar };
});
