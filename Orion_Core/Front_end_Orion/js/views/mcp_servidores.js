/* ==========================================================================
   ORION — views/mcp_servidores.js | servidores do mcp.json (E7.3) e prompts de MCP (E7.5)
   Card em Integrações: estado, protocolo, última falha, ferramentas com a classe de risco que VOCÊ
   deu, e "Testar" (só lista as ferramentas, não chama nenhuma). Os prompts dos servidores são seus:
   o texto vai para o campo de mensagem e só é enviado se você enviar (REGRAS.md: recurso e prompt de MCP são dado, não comando).
   Só `textContent`: nada do que o servidor diz vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, api, bus, ui } = O;

    let dados = null, erro = null, prompts = [], ativa = false;
    const testando = new Set(), resultados = new Map();
    const ROTULOS = { ok: 'Conectado', falhou: 'Falhou', 'incompatível': 'Protocolo incompatível', caiu: 'Caiu', desligado: 'Desligado', 'não': 'Não iniciou' };
    const novo = () => api.estado().backend === 'orion' && api.suporta('mcp_servers');
    const quando = ts => new Date(ts * 1000).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
    const dica = t => el('p', { class: 'extension-hint', text: t });

    async function testar(nome) {
        testando.add(nome); desenhar();
        try {
            const r = await api.mcpTestar(nome);
            resultados.set(nome, r.ok ? `Respondeu em ${r.ms} ms · ${r.ferramentas} ferramenta(s)${r.reiniciar ? ' · reinicie o Orion para as novas ferramentas entrarem' : ''}.` : `Não respondeu: ${r.erro || 'sem detalhe'}`);
        } catch (e) { resultados.set(nome, `Não consegui testar: ${e.message || ''}`.trim()); }
        testando.delete(nome);
        await carregar();
    }

    function servidorEl(s) {
        const ok = s.estado === 'ok';
        return el('article', { class: 'card mcp-servidor', dataset: { servidor: s.nome, estado: s.estado } },
            el('div', { class: 'card-head' }, el('h4', { class: 'card-title', text: s.nome }),
                el('span', { class: `badge ${ok ? 'badge-ok' : 'badge-muted'}`, text: ROTULOS[s.estado] || s.estado })),
            dica(`${s.transporte === 'remoto' ? 'Servidor remoto' : 'Servidor local'}${s.protocolo ? ` · protocolo ${s.protocolo}` : ''}${s.externo ? ' · conteúdo externo' : ''}`),
            ok ? null : el('p', { class: 'mcp-status', role: 'status', text: s.status }),
            s.ultima_falha ? dica(`Última falha (${quando(s.ultima_falha.quando)}): ${s.ultima_falha.mensagem}`) : null,
            s.ferramentas.length ? el('details', {}, el('summary', { text: `${s.ferramentas.length} ferramenta(s)` }),
                el('ul', { class: 'painel-lista', 'aria-label': `Ferramentas de ${s.nome}` }, ...s.ferramentas.map(f => el('li', { class: 'painel-item' },
                    el('div', { class: 'painel-item-top' }, el('strong', { text: f.nome }), el('span', { class: 'badge badge-muted', text: f.risco }),
                        f.externo ? el('span', { class: 'badge badge-muted', text: 'externo' }) : null, f.egress ? el('span', { class: 'badge badge-muted', text: 'rede' }) : null))))) : null,
            s.recursos_permitidos.length ? dica(`Recursos que o modelo pode ler: ${s.recursos_permitidos.join(', ')}`) : null,
            el('div', { class: 'conh-acoes' },
                el('button', { type: 'button', class: 'btn btn-outline btn-sm', disabled: testando.has(s.nome) || s.estado === 'desligado', text: testando.has(s.nome) ? 'Testando…' : 'Testar',
                    'aria-label': `Testar o servidor ${s.nome}`, on: { click: () => testar(s.nome) } })),
            resultados.has(s.nome) ? el('p', { class: 'extension-hint', role: 'status', dataset: { teste: s.nome }, text: resultados.get(s.nome) }) : null);
    }

    function desenhar() {
        const raiz = $('#mcp-servidores-root');
        if (!raiz) return;
        if (!novo() || (!dados?.ativo && !erro)) { raiz.hidden = true; raiz.replaceChildren(); return; }
        raiz.hidden = false;
        raiz.replaceChildren(el('section', { class: 'card', 'aria-labelledby': 'mcp-serv-titulo' },
            el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: 'mcp-serv-titulo', text: 'Servidores MCP (mcp.json)' }),
                el('span', { class: 'card-sub', text: erro ? 'sem resposta' : `${dados.servidores.length} servidor(es)` })),
            erro ? dica(`Não consegui ler o estado: ${erro.message || ''}`.trim()) : el('div', { class: 'grid' }, ...dados.servidores.map(servidorEl))));
    }

    async function carregar() {
        if (!novo()) { dados = null; desenhar(); return; }
        try { dados = await api.mcpServidores(); erro = null; } catch (e) { erro = e; }
        desenhar();
        carregarPrompts();
    }

    async function carregarPrompts() {
        if (!novo() || !dados?.ativo) { prompts = []; return; }
        try { prompts = (await api.mcpPrompts()).prompts || []; } catch (_) { prompts = []; }
    }

    /** o prompt do servidor vai para o campo de mensagem; argumentos faltando são perguntados */
    async function trazerPrompt(p, dados0 = null) {
        const alvo = prompts.find(x => x.servidor === p.servidor && x.nome === p.nome);
        const argumentos = { ...(dados0 || {}) };
        for (const a of alvo?.argumentos || []) {
            if (argumentos[a.nome] !== undefined && argumentos[a.nome] !== '') continue;
            const v = await ui.perguntar({ titulo: `Prompt ${p.servidor}:${p.nome}`, rotulo: `${a.nome}${a.obrigatorio ? '' : ' (opcional)'}`, ok: 'Trazer', max: 4000 });
            if (v === null) return;
            argumentos[a.nome] = v;
        }
        try {
            const r = await api.mcpPrompt(p.servidor, p.nome, argumentos);
            O.composer.preencher(r.texto);
            ui.toast(`Prompt de ${p.servidor} no campo. Revise antes de enviar: o texto vem do servidor.`, { tipo: 'info', ms: 5200 });
        } catch (e) { ui.toast(`Não consegui trazer o prompt. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
    }

    O.mcpServidores = { prompts: () => prompts, trazerPrompt, recarregar: carregar };
    bus.on('view', v => { ativa = v === 'integracoes'; if (ativa) carregar(); });
    bus.on('capabilities', () => { dados = null; prompts = []; carregar(); });
})();
