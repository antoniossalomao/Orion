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

    let timer = null, dados = null, falhou = false, desde = 0;

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
            const servidos = L.resumoProvedores(e.provedores);
            return el('li', { class: 'painel-item', dataset: { endpoint: e.nome, estado: s.estado } },
                el('div', { class: 'painel-item-top' },
                    el('span', { class: 'status-dot', dataset: { state: s.estado === 'idle' ? '' : s.estado }, 'aria-hidden': 'true' }),
                    el('strong', { text: e.nome }),
                    el('span', { class: 'mono painel-modelo', text: e.camada && e.camada !== 'padrão' ? `${e.modelo} · ${e.camada}` : e.modelo }),
                    el('span', { class: `badge badge-${s.estado === 'idle' ? 'muted' : s.estado}`, text: s.rotulo })),
                el('small', { text: detalhe }),
                servidos ? el('small', { class: 'painel-provedores', text: `Serviu: ${servidos}` }) : null);
        })), rot ? el('p', { class: 'painel-top', 'data-roteamento': '' }, `Roteamento por tipo de tarefa: ${rot}`) : null);
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
                cartao('modelos', 'Modelos', 'desde que o Orion subiu; não é a cota do provedor', modelosEl(dados.modelos, dados.roteamento)),
                cartao('clis', 'CLIs oficiais', 'uso de hoje', clisEl(dados.clis)),
                cartao('aprovacoes', 'Aprovações', `${dados.aprovacoes?.pendentes ?? 0} pendente(s)`, aprovacoesEl(dados.aprovacoes)),
                cartao('decisoes', 'Política', `últimas ${dados.decisoes?.janela_h ?? 24} h`, decisoesEl(dados.decisoes)),
                cartao('sistema', 'Sistema', null, sistemaEl(dados))));
        $('#painel-quando').textContent = `Atualizado às ${U.hora(new Date(desde))}`;
    }

    async function atualizar() {
        try { dados = await api.painel(); falhou = false; desde = Date.now(); }
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
