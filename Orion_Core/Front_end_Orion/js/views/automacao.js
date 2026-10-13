/* ==========================================================================
   ORION — views/automacao.js | automação do dia a dia (E9) em Atividade
   Rotinas (regra 58), tarefa em segundo plano (regra 59), organizador de Downloads (regra 58) e
   "posso desligar?". Nada aqui executa por conta própria: cada botão é um pedido seu, e o que o
   modelo propõe volta para o campo de mensagem (onde vale a política de sempre).
   Só `textContent`: nada que vem do servidor vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, api, bus, ui } = O;
    const L = O.automacaoLogica;

    let rotinas = [], tarefas = [], downloads = null, erro = null, ativa = false, tempo = 0;
    const msg = {};   // última frase de cada bloco: rotina:<nome>, tarefa, downloads, desligar
    const dica = t => el('p', { class: 'extension-hint', text: t });
    const botao = (rotulo, fn, extra = {}) => el('button', { type: 'button', class: 'btn btn-outline btn-sm', text: rotulo, on: { click: fn }, ...extra });
    const novo = () => api.estado().backend === 'orion' && api.suporta('automacao');
    const aviso = (chave, texto) => { msg[chave] = texto; desenhar(); };

    async function agirRotina(nome, acao) {
        try {
            const r = await api.rotinaAcao(nome, acao);
            aviso(`rotina:${nome}`, acao === 'executar' ? L.resumoDaExecucao(r) : acao === 'conceder' ? 'Liberada.' : 'Liberação retirada.');
        } catch (e) { aviso(`rotina:${nome}`, e.message || 'Não consegui.'); }
        await carregar();
    }
    function rotinaEl(r) {
        const est = L.estadoDaRotina(r.estado);
        return el('article', { class: 'card', dataset: { rotina: r.nome, estado: r.estado } },
            el('div', { class: 'card-head' }, el('h4', { class: 'card-title', text: r.nome }), el('span', { class: `badge ${est.tom === 'ok' ? 'badge-ok' : 'badge-muted'}`, text: est.rotulo })),
            r.descricao ? dica(r.descricao) : null,
            dica(`${r.passos} passo(s) · gatilhos: ${L.gatilhosLegiveis(r.gatilhos)}${r.liberar.length ? ` · roda direto: ${r.liberar.join(', ')}` : ''}`),
            r.erro ? el('p', { class: 'mcp-status', role: 'status', text: r.erro }) : null,
            el('div', { class: 'conh-acoes' },
                botao(r.estado === 'concedida' ? 'Revogar' : 'Conceder', () => agirRotina(r.nome, r.estado === 'concedida' ? 'revogar' : 'conceder'), { disabled: !!r.erro, 'aria-label': `${r.estado === 'concedida' ? 'Revogar' : 'Conceder'} a rotina ${r.nome}` }),
                botao('Executar', () => agirRotina(r.nome, 'executar'), { disabled: r.estado !== 'concedida', 'aria-label': `Executar a rotina ${r.nome}` })),
            msg[`rotina:${r.nome}`] ? el('p', { class: 'extension-hint', role: 'status', text: msg[`rotina:${r.nome}`] }) : null);
    }

    async function criarTarefa(ev) {
        ev.preventDefault();
        const f = ev.target, objetivo = f.objetivo.value.trim();
        if (!objetivo) return;
        try {
            await api.criarTarefaBg({ objetivo, ...L.limitesDaTarefa(f.passos.value, f.minutos.value) });
            f.objetivo.value = ''; aviso('tarefa', 'Começou. O relatório aparece aqui quando terminar.');
        } catch (e) { aviso('tarefa', e.message || 'Não consegui começar.'); }
        await carregar();
    }
    async function decidirProposta(t, p, acao) {
        try {
            const r = await api.decidirPropostaBg(t.id, p.indice, acao);
            if (acao === 'aceitar' && r.texto) { O.composer.preencher(r.texto); ui.toast('A proposta está no campo de mensagem. Revise e envie: aí vale a política de sempre.', { tipo: 'info', ms: 5200 }); }
        } catch (e) { aviso('tarefa', e.message || 'Não consegui.'); }
        await carregar();
    }
    function tarefaEl(t) {
        const abertas = L.propostasAbertas(t);
        return el('article', { class: 'card', dataset: { tarefa: String(t.id), status: t.status } },
            el('div', { class: 'card-head' }, el('h4', { class: 'card-title', text: t.objective }), el('span', { class: `badge ${t.status === 'concluida' ? 'badge-ok' : 'badge-muted'}`, text: L.statusDaTarefa(t.status) })),
            t.report ? el('details', {}, el('summary', { text: 'Relatório' }), el('pre', { class: 'extension-hint', text: t.report })) : null,
            ...abertas.map(p => el('div', { class: 'extension-actions', dataset: { proposta: String(p.indice) } },
                el('span', { class: 'extension-hint', text: p.texto }),
                botao('Trazer para o campo', () => decidirProposta(t, p, 'aceitar')), botao('Descartar', () => decidirProposta(t, p, 'descartar')))),
            t.status === 'rodando' ? botao('Cancelar', async () => { try { await api.cancelarTarefaBg(t.id); } catch (e) { aviso('tarefa', e.message || ''); } await carregar(); }) : null);
    }

    async function agirDownloads(acao, params) {
        try {
            const r = acao === 'desfazer' ? await api.desfazerDownloads(params.escopo) : await api.downloadsAcao(acao, params);
            aviso('downloads', acao === 'rodar' ? L.resumoDeDownloads(r) : acao === 'desfazer' ? `Voltou ${r.desfeitos.length} arquivo(s)${r.pulados.length ? ` · ${r.pulados.length} ficaram como estavam` : ''}.` : acao === 'conceder' ? 'Regras liberadas.' : 'Liberação retirada.');
        } catch (e) { aviso('downloads', e.message || 'Não consegui.'); }
        await carregar();
    }
    function downloadsEl() {
        const est = L.estadoDaRotina(downloads.erro ? 'erro' : downloads.estado);
        return el('section', { class: 'card', 'aria-labelledby': 'dl-titulo' },
            el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: 'dl-titulo', text: 'Organizador de Downloads' }), el('span', { class: 'badge badge-muted', text: downloads.existe ? est.rotulo : 'sem regras' })),
            dica(downloads.existe ? `${downloads.regras} regra(s) em ${downloads.pasta}` : 'Crie regras_downloads.yaml na pasta de dados (orion downloads exemplo).'),
            downloads.erro ? el('p', { class: 'mcp-status', role: 'status', text: downloads.erro }) : null,
            el('div', { class: 'conh-acoes' },
                botao(downloads.estado === 'concedida' ? 'Revogar' : 'Conceder', () => agirDownloads(downloads.estado === 'concedida' ? 'revogar' : 'conceder', {}), { disabled: !downloads.existe || !!downloads.erro }),
                botao('Simular', () => agirDownloads('rodar', { simular: true }), { disabled: downloads.estado !== 'concedida' }),
                botao('Organizar agora', () => agirDownloads('rodar', {}), { disabled: downloads.estado !== 'concedida' }),
                botao('Desfazer o último', () => agirDownloads('desfazer', { escopo: 'ultimo' }), { disabled: !downloads.historico?.length })),
            msg.downloads ? el('p', { class: 'extension-hint', role: 'status', text: msg.downloads }) : null);
    }

    async function conferirDesligar() {
        aviso('desligar', 'Conferindo…');
        try { aviso('desligar', L.resumoDeDesligar(await api.possoDesligar())); } catch (e) { aviso('desligar', e.message || 'Não consegui conferir.'); }
    }

    /** o formulário é criado uma vez: redesenhar a tela (a cada clique ou a cada 4 s) não apaga o que você digitou */
    let formEl = null;
    function formulario() {
        if (!formEl) {
            formEl = el('form', { class: 'extension-actions', autocomplete: 'off', on: { submit: criarTarefa } },
                el('label', { class: 'extension-field' }, el('span', { text: 'Objetivo (o Orion só lê e propõe)' }), el('input', { class: 'input', name: 'objetivo', maxlength: '500', placeholder: 'ex.: ver o que mudou nos meus repositórios esta semana' })),
                el('label', { class: 'extension-field' }, el('span', { text: 'Passos' }), el('input', { class: 'input', name: 'passos', type: 'number', min: '1', max: '30', value: '10' })),
                el('label', { class: 'extension-field' }, el('span', { text: 'Minutos' }), el('input', { class: 'input', name: 'minutos', type: 'number', min: '1', max: '30', value: '10' })),
                el('button', { type: 'submit', class: 'btn btn-primary btn-sm', text: 'Começar' }));
        }
        formEl.querySelector('button[type=submit]').disabled = !L.podeCriarTarefa(tarefas);
        return formEl;
    }

    function desenhar() {
        const raiz = $('#automacao-root');
        if (!raiz) return;
        if (!novo()) { raiz.hidden = true; raiz.replaceChildren(); return; }
        raiz.hidden = false;
        const form = formulario();
        raiz.replaceChildren(
            el('section', { class: 'card', 'aria-labelledby': 'auto-rot' },
                el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: 'auto-rot', text: 'Rotinas' }), el('span', { class: 'card-sub', text: erro ? 'sem resposta' : `${rotinas.length} rotina(s)` })),
                erro ? dica(`Não consegui ler: ${erro.message || ''}`.trim()) : rotinas.length ? el('div', { class: 'grid' }, ...rotinas.map(rotinaEl)) : dica('Nenhuma rotina em <dados>/rotinas. Veja `orion rotina exemplo`.')),
            el('section', { class: 'card', 'aria-labelledby': 'auto-bg' },
                el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: 'auto-bg', text: 'Tarefa em segundo plano' })),
                form, msg.tarefa ? el('p', { class: 'extension-hint', role: 'status', text: msg.tarefa }) : null,
                tarefas.length ? el('div', { class: 'grid' }, ...tarefas.slice(0, 5).map(tarefaEl)) : dica('Nenhuma tarefa ainda.')),
            downloads ? downloadsEl() : null,
            el('section', { class: 'card', 'aria-labelledby': 'auto-des' },
                el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: 'auto-des', text: 'Posso desligar?' })),
                el('div', { class: 'conh-acoes' }, botao('Conferir', conferirDesligar)),
                msg.desligar ? el('p', { class: 'extension-hint', role: 'status', dataset: { desligar: '1' }, text: msg.desligar }) : null));
    }

    async function carregar() {
        if (!novo()) { desenhar(); return; }
        try { rotinas = (await api.rotinas()).rotinas; erro = null; } catch (e) { erro = e; }
        try { tarefas = (await api.tarefasBg()).tarefas; } catch (_) { tarefas = []; }
        try { downloads = await api.downloads(); } catch (_) { downloads = null; }
        desenhar();
        clearTimeout(tempo);
        if (ativa && tarefas.some(t => t.status === 'rodando')) tempo = setTimeout(carregar, 4000);
    }

    async function carregarRotinas() {
        if (!novo()) return;
        try { rotinas = (await api.rotinas()).rotinas; } catch (_) { /* a tela mostra o erro quando abrir */ }
    }
    O.automacao = { recarregar: carregar, rotinas: () => rotinas };
    bus.on('view', v => { ativa = v === 'atividade'; if (ativa) carregar(); else clearTimeout(tempo); });
    bus.on('capabilities', () => { rotinas = []; tarefas = []; downloads = null; if (ativa) carregar(); else { desenhar(); carregarRotinas(); } });
})();
