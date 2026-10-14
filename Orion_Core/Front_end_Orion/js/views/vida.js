/* ==========================================================================
   ORION — views/vida.js | Vida pessoal (E10): finanças, preços, linha do tempo, sonhos e foco
   Tudo local (regra 61). Fatura: você cola o texto, revisa as linhas e só então lança. Foco só
   pergunta (regra 62). Só `textContent`. Lógica pura em vida_logica.js.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, api, ui } = O;
    const L = window.OrionVidaLogica;

    let geracao = 0;
    const vazio = t => el('p', { class: 'painel-vazio', role: 'status', text: t });
    const linha = (...filhos) => el('div', { class: 'vida-linha' }, filhos);
    const botao = (texto, fn) => el('button', { type: 'button', class: 'btn btn-outline btn-sm', text: texto, on: { click: fn } });
    const erroDe = (e, acao) => ui.toast(`${acao}. ${e.message || ''}`.trim(), { tipo: 'erro' });

    async function carregarFinancas(corpo, g) {
        const [resumo, contas] = await Promise.all([api.financasResumo(), api.financasContas()]);
        if (g !== geracao) return;
        corpo.querySelector('#vida-resumo').replaceChildren(
            el('p', { text: `Gastos do mês: ${L.reais(resumo.total_cents)}` }),
            ...Object.entries(resumo.por_categoria || {}).map(([k, v]) => el('p', { class: 'extension-hint', text: `${k}: ${L.reais(v)}` })),
        );
        const lista = corpo.querySelector('#vida-contas');
        lista.replaceChildren(...((contas.contas || []).length
            ? contas.contas.map(c => linha(el('span', { text: `${c.description} · ${L.reais(c.cents)} · vence ${c.due}` }),
                botao('Paguei', async () => {
                    try { await api.financasPagar(c.id); ui.toast('Conta paga e lançada.'); carregarFinancas(corpo, geracao); }
                    catch (e) { erroDe(e, 'Não consegui marcar a conta'); }
                })))
            : [vazio('Nenhuma conta aberta.')]));
    }

    function fatura(corpo) {
        const area = corpo.querySelector('#vida-fatura-texto');
        const saida = corpo.querySelector('#vida-fatura-saida');
        corpo.querySelector('#vida-fatura-ler').addEventListener('click', async () => {
            try {
                const r = await api.financasPrevia(area.value);
                saida.replaceChildren(
                    el('p', { text: `${r.linhas.length} linha(s), total ${L.reais(r.total_cents)}. Confira antes de lançar.` }),
                    ...r.linhas.map(x => el('p', { class: 'extension-hint', text: `${x.data} · ${x.descricao} · ${L.reais(x.cents)}` })),
                    r.linhas.length ? botao(`Lançar ${r.linhas.length} gasto(s)`, async () => {
                        const ok = await ui.confirmar({ titulo: 'Lançar esta fatura?', texto: 'Os gastos entram no seu controle local.', ok: 'Lançar' });
                        if (!ok) return;
                        try { await api.financasConfirmarFatura(r.linhas); ui.toast('Fatura lançada.'); area.value = ''; saida.replaceChildren(); carregarFinancas(corpo, geracao); }
                        catch (e) { erroDe(e, 'Não consegui lançar'); }
                    }) : vazio('Nenhuma linha reconhecida.'),
                );
            } catch (e) { erroDe(e, 'Não consegui ler a fatura'); }
        });
    }

    function simulador(corpo) {
        const campo = id => corpo.querySelector(`#${id}`);
        corpo.querySelector('#vida-sim-rodar').addEventListener('click', async () => {
            try {
                const r = await api.simularCenarios({
                    saldo_reais: Number(campo('vida-sim-saldo').value || 0),
                    meses: Number(campo('vida-sim-meses').value || 1),
                    extra_mensal_reais: Number(campo('vida-sim-extra').value || 0),
                });
                campo('vida-sim-saida').textContent = `${r.meses} mês(es): ${L.faixas(r.faixas_cents)}. ${r.aviso}.`;
            } catch (e) { erroDe(e, 'Não consegui simular'); }
        });
    }

    async function carregarPrecos(corpo) {
        const r = await api.precosLista();
        corpo.querySelector('#vida-precos').replaceChildren(...((r.itens || []).length
            ? r.itens.map(p => el('p', { text: `${p.item}${p.ultimo_cents != null ? ` · último ${L.reais(p.ultimo_cents)}` : ''}${p.alvo_cents != null ? ` · alvo ${L.reais(p.alvo_cents)}` : ''}` }))
            : [vazio('Nada acompanhado.')]));
    }

    async function carregarLinha(corpo, zoom) {
        const { de, ate } = L.janela(zoom, Date.now() / 1000);
        const r = await api.linhaDoTempo(de, ate);
        const alvo = corpo.querySelector('#vida-tempo');
        const grupos = L.agrupaPorDia(r.itens || []);
        alvo.replaceChildren(...(grupos.length ? grupos.flatMap(g => [
            el('h3', { text: g.dia }),
            ...g.itens.map(i => el('p', { class: 'extension-hint', text: `${new Date(i.ts * 1000).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })} · ${L.rotuloTipo(i.tipo)} · ${i.texto}` })),
        ]) : [vazio('Nada registrado nesse período.')]));
    }

    async function carregarSonhos(corpo) {
        const avisos = await api.req('/notifications', { timeout: 6000 });
        const sonhos = (avisos || []).filter(a => a.kind === 'sonho');
        corpo.querySelector('#vida-sonhos').replaceChildren(...(sonhos.length ? sonhos.map(s => linha(
            el('span', { text: String(s.text || '').replace(/^Sonho do dia: /, '') }),
            botao('boa', async () => { try { await api.sonhoBoa(s.id); ui.toast('Gravado como fato.'); carregarSonhos(corpo); } catch (e) { erroDe(e, 'Não consegui gravar'); } }),
            botao('descartar', async () => { try { await api.sonhoDescartar(s.id); carregarSonhos(corpo); } catch (e) { erroDe(e, 'Não consegui descartar'); } }),
        )) : [vazio('Nenhum sonho pendente.')]));
    }

    async function carregarFoco(corpo) {
        const f = await api.focoEstado();
        corpo.querySelector('#vida-foco-estado').textContent = f.ativo ? 'Bloco de foco ativo. Eu só pergunto se o app em foco for distração.' : 'Sem bloco de foco.';
    }

    function montar() {
        const corpo = $('#vida-corpo');
        corpo.replaceChildren(
            el('section', { class: 'card', 'aria-label': 'Finanças' }, [
                el('h3', { text: 'Finanças (só neste computador)' }), el('div', { id: 'vida-resumo' }),
                el('h4', { text: 'Contas em aberto' }), el('div', { id: 'vida-contas' }),
                el('h4', { text: 'Ler fatura' }),
                el('label', { class: 'sr-only', for: 'vida-fatura-texto', text: 'Texto da fatura' }),
                el('textarea', { id: 'vida-fatura-texto', class: 'input', rows: '5', placeholder: 'Cole o texto da fatura (DD/MM descrição valor)' }),
                el('button', { id: 'vida-fatura-ler', type: 'button', class: 'btn btn-outline btn-sm', text: 'Ler fatura' }), el('div', { id: 'vida-fatura-saida' }),
                el('h4', { text: 'Simular cenário' }),
                el('input', { id: 'vida-sim-saldo', class: 'input', type: 'number', placeholder: 'Saldo (R$)' }),
                el('input', { id: 'vida-sim-meses', class: 'input', type: 'number', placeholder: 'Meses', value: '6' }),
                el('input', { id: 'vida-sim-extra', class: 'input', type: 'number', placeholder: 'Extra mensal (R$)', value: '0' }),
                el('button', { id: 'vida-sim-rodar', type: 'button', class: 'btn btn-outline btn-sm', text: 'Simular' }), el('p', { id: 'vida-sim-saida', class: 'extension-hint' }),
            ]),
            el('section', { class: 'card', 'aria-label': 'Preços' }, [
                el('h3', { text: 'Monitor de preço' }), el('div', { id: 'vida-precos' }),
            ]),
            el('section', { class: 'card', 'aria-label': 'Linha do tempo' }, [
                el('h3', { text: 'Linha do tempo' }),
                el('select', { id: 'vida-zoom', class: 'select' }, [el('option', { value: 'semana', text: 'Semana' }), el('option', { value: 'dia', text: 'Dia' })]),
                el('div', { id: 'vida-tempo' }),
            ]),
            el('section', { class: 'card', 'aria-label': 'Sonhos' }, [
                el('h3', { text: 'Sonho do dia' }), el('div', { id: 'vida-sonhos' }),
            ]),
            el('section', { class: 'card', 'aria-label': 'Escudo de foco' }, [
                el('h3', { text: 'Escudo de foco' }), el('p', { id: 'vida-foco-estado', class: 'extension-hint' }),
                el('input', { id: 'vida-foco-min', class: 'input', type: 'number', value: '25', placeholder: 'Minutos' }),
                el('button', { id: 'vida-foco-iniciar', type: 'button', class: 'btn btn-outline btn-sm', text: 'Iniciar bloco' }),
            ]),
        );
        return corpo;
    }

    async function ativar() {
        const g = ++geracao;
        const corpo = montar();
        fatura(corpo); simulador(corpo);
        corpo.querySelector('#vida-zoom').addEventListener('change', e => carregarLinha(corpo, e.target.value));
        corpo.querySelector('#vida-foco-iniciar').addEventListener('click', async () => {
            try { await api.focoIniciar(Number(corpo.querySelector('#vida-foco-min').value || 25)); ui.toast('Bloco de foco iniciado.'); carregarFoco(corpo); }
            catch (e) { erroDe(e, 'Não consegui iniciar o foco'); }
        });
        for (const tarefa of [carregarFinancas(corpo, g), carregarPrecos(corpo), carregarLinha(corpo, 'semana'), carregarSonhos(corpo), carregarFoco(corpo)]) {
            tarefa.catch(e => erroDe(e, 'Não consegui carregar a Vida'));
        }
    }

    function desativar() { geracao++; }

    O.views.vida = { ativar, desativar };
})();
