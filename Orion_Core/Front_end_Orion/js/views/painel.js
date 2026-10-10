/* ==========================================================================
   ORION — views/painel.js | painel único: modelos, CLIs, aprovações, política e sistema
   Tudo vem de `GET /painel` (orion/painel.py); a lógica de estados e alertas é pura e fica em
   js/painel.js (testada em Node). Só `textContent`: nome de modelo ou de ferramenta vindo do
   servidor nunca vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, api } = O;
    const U = O.util;
    const L = O.painelLogica;

    let timer = null, dados = null, atividade = null, falhou = false, desde = 0;

    const tempo = ts => (ts ? U.hora(new Date(ts * 1000)) : '—');

    function cartao(id, titulo, dica, ...filhos) {
        return el('section', { class: 'card painel-card', 'aria-labelledby': `pn-${id}`, dataset: { id } },
            el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: `pn-${id}`, text: titulo }),
                dica ? el('span', { class: 'card-sub', text: dica }) : null),
            ...filhos);
    }

    const vazio = texto => el('p', { class: 'painel-vazio', text: texto });
    const linha = (rotulo, valor) => el('div', { class: 'painel-kv' },
        el('dt', { text: rotulo }), el('dd', { class: 'mono', text: valor }));

    function alertasEl(a) {
        if (!a.length) return el('div', { class: 'banner banner-info', role: 'status' }, el('span', { html: icone('check') }),
            el('div', {}, el('strong', { text: 'Tudo em ordem. ' }), 'Nada pede a sua atenção agora.'));
        return el('div', { class: 'painel-alertas', role: 'status' }, ...a.map(x => el('div', { class: `banner banner-${x.nivel}` },
            el('span', { html: icone('alert') }), el('div', {}, el('strong', { text: x.nivel === 'danger' ? 'Atenção: ' : 'Aviso: ' }), x.texto))));
    }

    function modelosEl(m, roteamento) {
        if (!m?.configurado) return vazio('O gateway de modelos não está configurado: o chat fica desligado.');
        if (!m.endpoints?.length) return vazio('Sem números de uso neste gateway.');
        const rot = L.resumoRoteamento(roteamento);
        return el('div', {}, el('ul', { class: 'painel-lista' }, ...m.endpoints.map(e => {
            const s = L.estadoModelo(e);
            const detalhe = `${e.ok} ok · ${e.falhas} falha(s)${e.limitada ? ` · ${e.limitada}× cota` : ''}`
                + (e.ultimo_erro ? ` · último erro: ${e.ultimo_erro}` : '') + (e.ultimo_ok ? ` · última resposta ${tempo(e.ultimo_ok)}` : '');
            const adiado = Number(e.orcamento) > 0 ? `Limite do dia gasto: ${Number(e.orcamento)}× para o fim da fila` : '';
            return el('li', { class: 'painel-item', dataset: { endpoint: e.nome, estado: s.estado } },
                el('div', { class: 'painel-item-top' },
                    el('span', { class: 'status-dot', dataset: { state: s.estado === 'idle' ? '' : s.estado }, 'aria-hidden': 'true' }),
                    el('strong', { text: e.nome }),
                    el('span', { class: 'mono painel-modelo', text: e.camada && e.camada !== 'padrão' ? `${e.modelo} · ${e.camada}` : e.modelo }),
                    el('span', { class: `badge badge-${s.estado === 'idle' ? 'muted' : s.estado}`, text: s.rotulo })),
                el('small', { text: detalhe }),
                adiado ? el('small', { class: 'painel-orcamento', text: adiado }) : null);
        })), rot ? el('p', { class: 'painel-top', 'data-roteamento': '' }, `Roteamento por tipo de tarefa: ${rot}`) : null);
    }

    /** barras simples (uma por dia) das respostas dos últimos 7 dias; só textContent e CSS */
    function semanaEl(semana) {
        if (!semana?.some(d => d.total || d.erros)) return vazio('Ainda sem respostas contadas nesta semana.');
        const maior = Math.max(1, ...semana.map(d => d.total));
        const total = semana.reduce((a, d) => a + d.total, 0), erros = semana.reduce((a, d) => a + d.erros, 0);
        return el('div', {}, el('ul', { class: 'painel-semana', 'aria-label': 'Respostas por dia' }, ...semana.map(d => {
            const rotulo = `${d.dia.slice(6)}/${d.dia.slice(4, 6)}`;
            return el('li', { class: 'painel-dia', dataset: { dia: d.dia, total: String(d.total) } },
                el('span', { class: 'painel-barra', style: `height:${Math.round((d.total / maior) * 100)}%`, 'aria-hidden': 'true' }),
                el('span', { class: 'painel-dia-n mono', text: String(d.total) }),
                el('span', { class: 'painel-dia-r', text: rotulo }),
                el('span', { class: 'sr-only', text: `${rotulo}: ${d.total} resposta(s)${d.erros ? `, ${d.erros} falha(s)` : ''}` }));
        })), el('p', { class: 'painel-top', 'data-semana': '' }, `${total} resposta(s) em 7 dias · ${erros} falha(s) sem resposta · não é a cota do provedor`));
    }

    /** caixa de atividade: avisos do Orion (briefing, lembretes, relatórios...) mesmo sem Telegram */
    function atividadeEl(a) {
        if (!a) return vazio('Consultando…');
        if (!a.avisos.length) return vazio('Nenhum aviso ainda.');
        return el('ul', { class: 'painel-lista', 'aria-label': 'Avisos' }, ...a.avisos.slice(0, 10).map(n =>
            el('li', { class: 'painel-item', dataset: { aviso: String(n.id), lido: String(n.entregue) } },
                el('div', { class: 'painel-item-top' }, el('strong', { text: n.tipo }),
                    el('span', { class: `badge badge-${n.entregue ? 'muted' : 'warn'}`, text: n.entregue ? 'Lido' : 'Novo' })),
                el('small', { class: 'conh-aviso', text: n.texto.length > 280 ? `${n.texto.slice(0, 280)}…` : n.texto }),
                n.entregue ? null : el('div', { class: 'conh-acoes' }, el('button', { class: 'btn btn-outline btn-sm', type: 'button',
                    'aria-label': `Marcar aviso ${n.tipo} como lido`, text: 'Marcar como lido',
                    on: { click: async () => { try { await api.lerAviso(n.id); } catch (_) { /* a lista real vem no próximo refresh */ } atualizar(); } } })))));
    }

    function clisEl(clis) {
        if (!clis?.length) return vazio('Nenhuma CLI oficial configurada para delegar tarefas.');
        return el('div', { class: 'painel-meters' }, ...clis.map(c => {
            const u = L.usoCli(c);
            // role="meter" exige um valor: sem a CLI instalada não há o que medir
            const medido = c.instalada ? { role: 'meter', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(u.pct) } : {};
            return el('div', { class: 'meter painel-meter', dataset: { sev: u.sev, cli: c.nome }, 'aria-label': `${c.nome}: ${u.texto}`, ...medido },
                el('span', { text: c.nome }), el('span', { class: 'meter-track' }, el('span', { class: 'meter-fill', style: `width:${u.pct}%` })),
                el('span', { class: 'meter-val', text: u.valor }), el('small', { class: 'painel-meter-nota', text: u.texto }));
        }));
    }

    /** Cota gratuita que o Orion contou hoje (regra 46), no mesmo medidor das CLIs */
    function cotaEl(cota) {
        if (!cota?.length) return vazio('Sem conta de cota neste cérebro.');
        return el('div', { class: 'painel-meters' }, ...cota.map(c => {
            const u = L.usoCota(c);
            const medido = c.limite ? { role: 'meter', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(u.pct) } : {};
            return el('div', { class: 'meter painel-meter', dataset: { sev: u.sev, cota: c.provedor }, 'aria-label': `${c.nome}: ${u.valor}, ${u.texto}`, ...medido },
                el('span', { text: c.nome }), el('span', { class: 'meter-track' }, el('span', { class: 'meter-fill', style: `width:${u.pct}%` })),
                el('span', { class: 'meter-val', text: u.valor }), el('small', { class: 'painel-meter-nota', text: u.texto }));
        }));
    }

    /** Provedores na semana (registro de saída, regra 47): chamadas, falhas, latência e tráfego */
    function provedoresEl(lista) {
        const linhas = L.linhasProvedores(lista);
        if (!linhas.length) return vazio('Nenhuma chamada para fora nesta semana.');
        const th = texto => el('th', { scope: 'col', text: texto });
        // rolável na horizontal: precisa de foco pelo teclado (axe: scrollable-region-focusable)
        return el('div', { class: 'tabela-rola', tabindex: '0', role: 'region', 'aria-label': 'Provedores na semana' }, el('table', { class: 'tabela painel-provedores-tab' },
            el('caption', { class: 'sr-only', text: 'Chamadas por provedor nos últimos 7 dias' }),
            el('thead', {}, el('tr', {}, th('Provedor'), th('Tipo'), th('Chamadas'), th('Falhas'), th('Latência'), th('Modelo'), th('Tráfego'))),
            el('tbody', {}, ...linhas.map(x => el('tr', { dataset: { provedor: x.provedor, tipo: x.tipo } },
                el('th', { scope: 'row', class: 'mono', text: x.provedor }), el('td', { text: x.tipo }),
                el('td', { class: 'mono', text: String(x.chamadas) }), el('td', { class: 'mono', text: String(x.falhas) }),
                el('td', { class: 'mono', text: x.latencia }), el('td', { class: 'mono', text: x.modelo }),
                el('td', { class: 'mono', text: x.trafego }))))));
    }

    /** Pânico e não perturbe (regra 48): entrar no pânico é um clique; sair pede a senha de novo */
    function controleEl(m) {
        if (!m) return vazio('Este cérebro não tem modo pânico.');
        const r = L.resumoModos(m);
        const botao = (texto, fn, extra = {}) => el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: texto, on: { click: fn }, ...extra });
        return el('div', {},
            el('dl', { class: 'painel-dl' },
                el('div', { class: 'painel-kv', dataset: { modo: 'panico', estado: m.panico ? 'ligado' : 'desligado' } },
                    el('dt', { text: 'Modo pânico' }), el('dd', { text: r.panico })),
                el('div', { class: 'painel-kv', dataset: { modo: 'nao-perturbe', estado: m.nao_perturbe ? 'ligado' : 'desligado' } },
                    el('dt', { text: 'Não perturbe' }), el('dd', { text: r.dnd }))),
            el('div', { class: 'conh-acoes' },
                m.panico ? botao('Sair do modo pânico', sairDoPanico) : botao('Ligar modo pânico', entrarNoPanico, { class: 'btn btn-danger btn-sm' }),
                m.nao_perturbe_ate ? botao('Desligar não perturbe', () => naoPerturbe(null)) : botao('Não perturbe até…', perguntarNaoPerturbe)));
    }

    async function entrarNoPanico() {
        const sim = await O.ui.confirmar({ titulo: 'Ligar o modo pânico?', ok: 'Ligar', perigo: true,
            texto: 'Corta as ferramentas de rede e de execução, a memória da tela, a escuta e os jobs que usam rede. Nada volta sozinho: para sair, a senha é pedida de novo.' });
        if (!sim) return;
        try { await api.panico(true); O.ui.toast('Modo pânico ligado.', { tipo: 'aviso' }); }
        catch (e) { O.ui.toast(`Não consegui ligar: ${e.message}`, { tipo: 'erro' }); }
        atualizar();
    }

    async function sairDoPanico() {
        const senha = await O.ui.perguntar({ titulo: 'Sair do modo pânico', rotulo: 'Confirme a senha', ok: 'Sair', senha: true, max: 256 });
        if (senha == null) return;
        try { await api.panico(false, senha); O.ui.toast('Modo pânico desligado.', { tipo: 'ok' }); }
        catch (e) { O.ui.toast(e.message, { tipo: 'erro' }); }
        atualizar();
    }

    async function perguntarNaoPerturbe() {
        const ate = await O.ui.perguntar({ titulo: 'Não perturbe', rotulo: 'Até que horas? (HH:MM)', valor: '07:00', ok: 'Ligar', max: 5 });
        if (ate == null) return;
        if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(ate)) { O.ui.toast('Use o formato HH:MM, por exemplo 07:00.', { tipo: 'aviso' }); return; }
        naoPerturbe(ate);
    }

    async function naoPerturbe(ate) {
        try { await api.naoPerturbe(ate); O.ui.toast(ate ? `Não perturbe até ${ate}.` : 'Não perturbe desligado.', { ms: 2200 }); }
        catch (e) { O.ui.toast(`Não consegui mudar: ${e.message}`, { tipo: 'erro' }); }
        atualizar();
    }

    function aprovacoesEl(a) {
        if (!a?.pendentes) return vazio('Nenhuma ação esperando aval.');
        return el('div', {}, el('ul', { class: 'painel-lista' }, ...a.itens.map(i => el('li', { class: 'painel-item' },
            el('div', { class: 'painel-item-top' }, el('strong', { class: 'mono', text: i.ferramenta }),
                el('span', { class: 'badge badge-warn', text: `expira em ${L.duracao(i.expira_em_s)}` })),
            el('small', { text: i.motivo || 'Sem motivo informado.' })))),
        el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Abrir o chat para decidir', on: { click: () => O.app.ir('chat') } }));
    }

    function decisoesEl(d) {
        const ac = d?.por_acao || {};
        const num = (rotulo, n, tom) => el('div', { class: 'painel-num', dataset: { tom } },
            el('strong', { text: String(n ?? 0) }), el('span', { text: rotulo }));
        return el('div', {},
            el('p', { class: 'painel-resumo', text: L.resumoDecisoes(d) }),
            el('div', { class: 'painel-nums' }, num('liberadas', ac.allow, 'ok'), num('pediram aval', ac.confirm, 'warn'), num('negadas', ac.deny, 'danger')),
            d?.mais_usadas?.length ? el('p', { class: 'painel-top' }, el('span', { text: 'Mais usadas: ' }),
                el('span', { class: 'mono', text: d.mais_usadas.map(x => `${x.ferramenta} ×${x.n}`).join(' · ') })) : null,
            d?.recentes?.length ? el('ul', { class: 'painel-lista painel-recentes', 'aria-label': 'Decisões recentes' }, ...d.recentes.map(r => el('li', { class: 'painel-item' },
                el('div', { class: 'painel-item-top' }, el('span', { class: 'mono painel-hora', text: tempo(r.ts) }),
                    el('strong', { class: 'mono', text: r.ferramenta }),
                    el('span', { class: `badge badge-${L.tomAcao(r.acao)}`, text: L.rotuloAcao(r.acao) })),
                r.motivo ? el('small', { text: r.motivo }) : null))) : null);
    }

    function sistemaEl(p) {
        const mcp = Object.entries(p.mcp || {});
        return el('dl', { class: 'painel-dl' },
            linha('No ar há', L.duracao(p.uptime_s)),
            linha('Memória', p.memoria?.ok ? (p.memoria.vetores ? 'ok · busca por significado ligada' : 'ok · só palavra-chave') : 'sem resposta'),
            linha('Jobs', !p.jobs?.ativo ? 'desligados' : p.jobs.ultima_rodada ? `última rodada às ${tempo(p.jobs.ultima_rodada)}` : 'ainda sem rodada'),
            linha('Telegram', p.canais?.telegram ? 'conectado' : 'desligado'),
            ...(p.voz ? [linha('Voz por clique', L.resumoVoz(p.voz).clique), linha('Voz ao vivo', L.resumoVoz(p.voz).aoVivo),
                linha('Palavra de ativação', L.resumoVoz(p.voz).escuta)] : []),
            ...(p.voz?.escuta?.ouvindo ? [el('div', { class: 'painel-kv' }, el('dt', { text: 'Escuta do microfone' }),
                el('dd', {}, el('button', { class: 'btn btn-outline btn-sm', type: 'button', 'data-escuta': '',
                    text: p.voz.escuta.pausada ? 'Retomar escuta' : 'Pausar escuta',
                    on: { click: async e => { e.currentTarget.disabled = true; try { await api.escutaAtivar(!!p.voz.escuta.pausada); } catch (_) { /* o painel mostra o estado real */ } atualizar(); } } })))] : []),
            linha('Ferramentas', String(p.ferramentas ?? 0)),
            linha('Avisos na fila', String(p.avisos?.pendentes ?? 0)),
            ...(mcp.length ? mcp.map(([n, s]) => linha(`MCP · ${n}`, s)) : [linha('MCP', 'nenhum servidor')]));
    }

    function desenhar() {
        const raiz = $('#painel-corpo');
        raiz.setAttribute('aria-busy', 'false');
        if (!dados) {
            raiz.replaceChildren(el('div', { class: 'banner banner-warn', role: 'status' }, el('span', { html: icone('alert') }),
                el('div', {}, el('strong', { text: falhou ? 'O cérebro não respondeu. ' : 'Consultando… ' },
                ), falhou ? 'Confira a conexão em Configurações › Conexão e atualize.' : '')));
            return;
        }
        const a = L.alertas(dados);
        if (falhou) a.unshift({ nivel: 'warn', texto: 'A última atualização falhou: os números abaixo podem estar velhos.' });
        raiz.replaceChildren(
            alertasEl(a),
            el('div', { class: 'grid grid-2 painel-grade' },
                cartao('controle', 'Controle', dados.modos?.panico ? 'Modo pânico' : null, controleEl(dados.modos)),
                cartao('cota', 'Cota de hoje', 'o que o Orion contou; não é o painel do provedor', cotaEl(dados.cota)),
                cartao('modelos', 'Modelos', 'desde que o Orion subiu; não é a cota do provedor', modelosEl(dados.modelos, dados.roteamento)),
                cartao('provedores', 'Provedores', 'últimos 7 dias, pelo registro de saída', provedoresEl(dados.provedores)),
                cartao('atividade', 'Atividade', atividade ? `${atividade.nao_lidos} não lido(s)` : null, atividadeEl(atividade)),
                cartao('semana', 'Uso da semana', 'respostas por dia', semanaEl(dados.semana)),
                cartao('clis', 'CLIs oficiais', 'uso de hoje', clisEl(dados.clis)),
                cartao('aprovacoes', 'Aprovações', `${dados.aprovacoes?.pendentes ?? 0} pendente(s)`, aprovacoesEl(dados.aprovacoes)),
                cartao('decisoes', 'Política', `últimas ${dados.decisoes?.janela_h ?? 24} h`, decisoesEl(dados.decisoes)),
                cartao('sistema', 'Sistema', null, sistemaEl(dados))));
        $('#painel-quando').textContent = `Atualizado às ${U.hora(new Date(desde))}`;
    }

    async function atualizar() {
        try {
            [dados, atividade] = await Promise.all([api.painel(), api.caixaDeAtividade().catch(() => atividade)]);
            falhou = false; desde = Date.now();
        }
        catch (_) { falhou = true; }
        desenhar();
    }

    O.views = O.views || {};
    O.views.painel = {
        init() {
            $('#painel-refresh').addEventListener('click', async e => {
                const b = e.currentTarget; b.disabled = true;
                await atualizar();
                b.disabled = false;
            });
        },
        ativar() {
            atualizar();
            clearInterval(timer);
            timer = setInterval(() => { if (!document.hidden) atualizar(); }, 10000);
        },
        desativar() { clearInterval(timer); timer = null; },
    };
})();
