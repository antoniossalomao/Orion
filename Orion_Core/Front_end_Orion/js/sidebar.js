/* ==========================================================================
   ORION — sidebar.js | conversas, busca, recolher, estado da conexão
   Barra fixa, recolhível a só ícones (Ctrl+B); em janela estreita (≤ 860 px) fica sempre recolhida.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, bus, ui, api, prefs } = O;
    const U = O.util;

    let sessoes = [], ativa = null, filtro = '', online = null, listaOnline = null, abrindo = false;
    let lista, busca, seletor, restaurado = false, origem = null;
    bus.on('capabilities', () => { const novaOrigem = `${api.base()}:${api.estado().backend}`; if (origem !== novaOrigem) { origem = novaOrigem; restaurado = false; if (busca) filtrar(); } });

    /* ── lista de conversas ────────────────────────────────────────────── */
    function titulo(s) { return s.titulo || 'Sem título'; }

    function comMarcas(texto, consulta) {
        if (!consulta) return [texto];
        const pos = new Set(O.fuzzy.contem(consulta, texto).marcas);
        if (!pos.size) return [texto];
        const saida = [];
        let acc = '', marcado = false;
        for (let i = 0; i < texto.length; i++) {
            const m = pos.has(i);
            if (m !== marcado && acc) { saida.push(marcado ? el('mark', { text: acc }) : acc); acc = ''; }
            marcado = m; acc += texto[i];
        }
        if (acc) saida.push(marcado ? el('mark', { text: acc }) : acc);
        return saida;
    }

    const linhas = new Map(), grupos = new Map();
    let menu, alvoMenu = null, resultadosBusca = null, proximoBusca = null, buscando = false, geracaoBusca = 0, erroBusca = null, maisBusca;
    async function buscarConteudo(g, mais = false) {
        if (!filtro || g !== geracaoBusca) return;
        buscando = true; erroBusca = null; desenhar();
        try {
            const d = await api.buscarSessoes(filtro, mais ? proximoBusca : 0);
            if (g !== geracaoBusca) return;
            resultadosBusca = mais ? [...resultadosBusca, ...d.sessoes] : d.sessoes;
            resultadosBusca = [...new Map(resultadosBusca.map(x => [x.sessao_id, x])).values()];
            proximoBusca = d.proximo_offset;
        } catch (e) { if (g === geracaoBusca) erroBusca = `Não consegui buscar conversas: ${e.message}`; }
        finally { if (g === geracaoBusca) { buscando = false; desenhar(); } }
    }
    const agendarBusca = U.debounce(buscarConteudo, 350);
    function filtrar() {
        filtro = busca.value.trim(); const g = ++geracaoBusca;
        agendarBusca.cancel(); erroBusca = null; proximoBusca = null; buscando = false;
        if (filtro && api.suporta('session_search')) { resultadosBusca = []; buscando = true; agendarBusca(g); }
        else resultadosBusca = null;
        desenhar();
    }
    function fecharMenu(devolver = true) {
        if (!menu || menu.hidden) return;
        menu.hidden = true;
        if (alvoMenu) { alvoMenu.mais.setAttribute('aria-expanded', 'false'); if (devolver) alvoMenu.mais.focus({ preventScroll: true }); }
    }
    function abrirMenu(linha) {
        fecharMenu(false); alvoMenu = linha;
        const s = linha.dados;
        menu.replaceChildren(...[
            ['renomear', 'Renomear'], ['fixar', s.favorita ? 'Desafixar' : 'Fixar'],
            ['arquivar', s.arquivada ? 'Restaurar' : 'Arquivar'],
            ...(api.suporta('projects') ? [['projeto', 'Mover para projeto']] : []),
        ].map(([valor, text]) => el('button', { type: 'button', role: 'menuitem', text, dataset: { valor } })));
        const r = linha.mais.getBoundingClientRect();
        menu.style.left = `${Math.max(8, Math.min(innerWidth - 180, r.right - 168))}px`;
        menu.style.top = `${Math.min(r.bottom + 4, innerHeight - 130)}px`;
        menu.hidden = false; linha.mais.setAttribute('aria-expanded', 'true'); menu.firstElementChild.focus();
    }
    async function gerenciar(linha, acao) {
        const s = linha.dados;
        if (acao === 'projeto') return O.projects.mover(s.sessao_id);
        try {
            let dados;
            if (acao === 'renomear') {
                const texto = await ui.confirmar({ titulo: 'Renomear conversa', ok: 'Salvar', campo: { valor: titulo(s), rotulo: 'Título da conversa' } });
                if (texto === false) return;
                dados = { titulo: texto };
            } else if (acao === 'fixar') dados = { favorita: !s.favorita };
            else {
                if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para arquivar.', { tipo: 'aviso' }); return; }
                dados = { arquivada: !s.arquivada };
            }
            const d = await api.editarSessao(s.sessao_id, dados);
            if (dados.arquivada && O.historico?.sessao() === s.sessao_id) await O.historico.abrir(s.sessao_id);
            await carregar();
            if (filtro && api.suporta('session_search')) await buscarConteudo(geracaoBusca);
            ui.toast(dados.titulo ? 'Conversa renomeada.' : dados.favorita != null ? (dados.favorita ? 'Conversa fixada.' : 'Conversa desafixada.') : d.arquivada ? 'Conversa arquivada; mensagens preservadas.' : 'Conversa restaurada.', { tipo: 'ok' });
        } catch (e) { ui.toast(`Não consegui atualizar a conversa: ${e.message}`, { tipo: 'erro' }); }
    }
    function botaoConversa(s) {
        let linha = linhas.get(s.sessao_id);
        if (!linha) {
            const nome = el('span', { class: 'conv-title' });
            const marca = el('span', { class: 'conv-ro' });
            const trecho = el('span', { class: 'conv-snippet', hidden: true });
            const selo = el('span', { class: 'conv-projeto', hidden: true });
            const b = el('button', { class: 'conv', type: 'button', dataset: { id: s.sessao_id } }, el('span', { class: 'conv-text' }, nome, trecho), selo, marca);
            const mais = el('button', { class: 'icon-btn conv-more', type: 'button', 'aria-haspopup': 'menu', 'aria-expanded': 'false', 'aria-controls': 'conv-menu', text: '⋯' });
            linha = { el: el('div', { class: 'conv-row' }, b, mais), botao: b, nome, trecho, marca, selo, mais, dados: s, desenho: '' };
            b.addEventListener('click', () => abrir(linha.dados));
            mais.addEventListener('click', () => abrirMenu(linha));
            mais.addEventListener('keydown', e => { if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); e.stopPropagation(); abrirMenu(linha); } });
            linhas.set(s.sessao_id, linha);
        }
        linha.dados = s;
        linha.trecho.textContent = s.trecho || ''; linha.trecho.hidden = !s.trecho;
        const proj = projetoDaConversa(s);
        linha.selo.hidden = !proj;
        if (proj) {
            linha.selo.textContent = O.conversas.nomeCurto(proj.name); linha.selo.title = `Projeto: ${proj.name}`;
            linha.selo.style.setProperty('--matiz', String(O.conversas.matiz(proj.name)));
        }
        const chave = `${titulo(s)}:${filtro}`;
        if (chave !== linha.desenho) { linha.nome.replaceChildren(...comMarcas(titulo(s), filtro)); linha.desenho = chave; }
        const atual = s.sessao_id === (O.historico?.sessao() || ativa);
        linha.botao.setAttribute('aria-current', String(atual)); linha.botao.title = titulo(s);
        linha.marca.textContent = s.favorita ? 'fixada' : s.somente_leitura ? 'leitura' : '';
        linha.mais.hidden = !api.suporta('session_management') || s.importada;
        linha.mais.setAttribute('aria-label', `Opções de ${titulo(s)}`);
        return linha.el;
    }
    let botaoArquivadas = null;
    function gerirArquivadas() {
        if (!botaoArquivadas) botaoArquivadas = el('button', { class: 'btn btn-ghost btn-sm conv-arquivadas', type: 'button', text: 'Gerenciar arquivadas', on: { click: () => O.app.ir('arquivadas') } });
        return botaoArquivadas;
    }
    function grupo(nome) {
        if (!grupos.has(nome)) grupos.set(nome, el('div', { class: 'conv-group', text: nome }));
        return grupos.get(nome);
    }
    function reconciliar(nos) {
        const foco = lista.contains(document.activeElement) ? document.activeElement : null;
        const top = lista.scrollTop, manter = new Set(nos);
        [...lista.children].filter(n => !manter.has(n)).forEach(n => n.remove());
        nos.forEach((n, i) => { if (lista.children[i] !== n) lista.insertBefore(n, lista.children[i] || null); });
        if (foco?.isConnected && document.activeElement !== foco) foco.focus({ preventScroll: true });
        if (foco && !foco.isConnected) busca.focus({ preventScroll: true });
        lista.scrollTop = top;
    }
    const projetos = () => O.projects?.lista?.() || [];
    function projetoDaConversa(s) {
        const id = O.conversas.projetoDe(s);
        return id ? projetos().find(p => p.id === id) || null : null;
    }
    function filtroAtual() { return O.conversas.filtroValido(prefs.get('filtro_projeto'), projetos()); }
    function montarSeletor() {
        const visivel = api.suporta('projects') && projetos().length > 0;
        seletor.hidden = !visivel;
        const ativos = projetos().filter(p => !p.archived);
        seletor.replaceChildren(el('option', { value: 'todos', text: 'Todos os projetos' }), el('option', { value: 'nenhum', text: 'Sem projeto' }),
            ...ativos.map(p => el('option', { value: p.id, text: p.name })));
        seletor.value = filtroAtual();
        if (!visivel) seletor.value = 'todos';
    }
    const visiveis = () => O.conversas.filtrar(sessoes, seletor?.hidden ? 'todos' : filtroAtual(), projetos());

    function desenhar() {
        lista.setAttribute('aria-busy', String(buscando));
        if (resultadosBusca !== null) {
            const nos = resultadosBusca.map(botaoConversa);
            if (!nos.length) nos.push(grupo(erroBusca || (buscando ? 'Buscando conversas…' : `Nada encontrado para “${filtro}”.`)));
            if (proximoBusca != null) { maisBusca.disabled = buscando; nos.push(maisBusca); }
            reconciliar(nos); return;
        }
        const doFiltro = visiveis();
        if (!doFiltro.length && sessoes.length) {
            reconciliar([grupo('Nenhuma conversa neste filtro.')]); lista.firstElementChild.className = 'sb-empty'; return;
        }
        if (!sessoes.length) {
            reconciliar([grupo(!api.suporta('sessions') && api.estado().api === 'online' ? 'Conversas salvas ainda indisponíveis neste backend.' : listaOnline === false ? 'Não foi possível carregar as conversas. Tentando novamente…' : 'Nenhuma conversa ainda. Comece uma nova.')]);
            lista.firstElementChild.className = 'sb-empty'; return;
        }
        if (filtro) {
            const achadas = O.fuzzy.buscar(doFiltro, filtro, titulo);
            reconciliar(achadas.length ? achadas.map(botaoConversa) : [grupo(`Nada encontrado para “${filtro}”.`)]); return;
        }
        const nos = [], fixadas = doFiltro.filter(x => x.favorita && !x.arquivada);
        if (fixadas.length) nos.push(grupo('Fixadas'), ...fixadas.map(botaoConversa));
        for (const g of U.agruparPorDia(doFiltro.filter(x => !x.arquivada && !x.favorita), s => s.criada)) nos.push(grupo(g.rotulo), ...g.itens.map(botaoConversa));
        const antigas = doFiltro.filter(x => x.arquivada);
        if (antigas.length) nos.push(grupo('Arquivadas'), ...antigas.map(botaoConversa), gerirArquivadas());
        reconciliar(nos);
    }

    async function carregar() {
        try {
            const d = await api.sessoes();
            sessoes = d.sessoes || [];
            ativa = d.ativa || sessoes.find(s => s.ativa)?.sessao_id || null;
            listaOnline = true;
        } catch (e) { listaOnline = false; if (e.indisponivel) { sessoes = []; ativa = null; } }
        desenhar();
        bus.emit('sessoes', { lista: sessoes, ativa: O.historico?.sessao() || ativa, ativaServidor: ativa });
        if (!restaurado && ativa && api.estado().backend === 'orion' && api.suporta('history') && !O.chat.ocupado()) {
            restaurado = true;
            try { await O.historico.abrir(ativa); } catch (e) { restaurado = false; }
        }
    }

    async function abrir(s) {
        if (abrindo) return;
        if (s.sessao_id === ativa && !s.somente_leitura && (!O.historico?.sessao() || (O.historico.sessao() === ativa && !O.historico.leitura()))) { O.app.ir('chat'); return; }
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para trocar de conversa.', { tipo: 'aviso' }); return; }
        abrindo = true; bus.emit('historico');
        try {
            let msgs;
            if (s.somente_leitura) {
                if (api.estado().backend !== 'orion') msgs = (await api.historico(s.sessao_id)).mensagens || [];
            }
            else {
                const d = await api.ativarSessao(s.sessao_id);
                if (d.erro) throw new Error(d.erro);
                msgs = d.mensagens || [];
            }
            if (api.estado().backend === 'orion') { await O.historico.abrir(s.sessao_id); restaurado = true; }
            else { O.chat.limpar(); O.chat.renderHistorico(msgs); }
            if (s.somente_leitura) O.chat.nota('Sessão antiga, somente leitura.');
            O.app.ir('chat');
            await carregar();
        } catch (e) {
            ui.toast(`Não consegui abrir a conversa: ${e.message}`, { tipo: 'erro' });
        } finally { abrindo = false; bus.emit('historico'); }
    }

    async function nova() {
        if (abrindo) return;
        if (!api.suporta('sessions')) { ui.toast('Conversas salvas ainda indisponíveis neste backend.', { tipo: 'aviso' }); return; }
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para começar outra conversa.', { tipo: 'aviso' }); return; }
        abrindo = true; bus.emit('historico'); O.app.ir('chat');
        try {
            const d = await api.novaSessao(O.projects?.current()?.id || null);
            if (d && d.erro) throw new Error(d.erro);
            O.chat.limpar();
            if (api.estado().backend === 'orion') { restaurado = true; await O.historico.abrir(d.sessao_id); }
            await carregar();
        } catch (e) {
            ui.toast(`Cérebro fora do ar: não deu para criar a conversa. ${e.rede ? '' : e.message}`.trim(), { tipo: 'erro' });
            }
        finally { abrindo = false; bus.emit('historico'); }
        if (O.app.view() === 'chat') O.composer.foco();
    }

    /* ── recolher ──────────────────────────────────────────────────────── */
    function atualizarToggle() {
        const rec = prefs.get('sb') === 'collapsed';
        const t = $('#sb-toggle');
        t.setAttribute('aria-expanded', String(!rec));
        t.setAttribute('aria-label', rec ? 'Expandir barra lateral' : 'Recolher barra lateral');
        t.dataset.tip = rec ? 'Expandir (Ctrl+B)' : 'Recolher (Ctrl+B)';
    }
    function alternar() {
        if (O.estreita()) return;      // em janela estreita a barra fica sempre recolhida
        prefs.set('sb', prefs.get('sb') === 'collapsed' ? 'expanded' : 'collapsed');
        atualizarToggle();
    }

    /* ── conexão ───────────────────────────────────────────────────────── */
    let verificando = false;
    async function verificar() {
        if (document.hidden || verificando) return;
        verificando = true;
        try {
            const r = await api.ping();
            const antes = online;
            online = r.ok;
            $('#conn-dot').dataset.state = r.ok ? (r.model === 'ready' ? 'ok' : 'warn') : 'danger';
            const texto = r.ok ? (r.incompatible ? 'Versão incompatível' : r.model === 'unavailable' ? 'Modelo indisponível' : r.model === 'unknown' ? 'API disponível' : 'Conectado') : 'Sem conexão';
            // O status só é anunciado quando muda, nunca a cada polling.
            if ($('#conn-text').textContent !== texto) {
                $('#conn-text').textContent = texto;
                // A conexão continua identificável quando só o trilho de ícones está visível.
                $('#conn').setAttribute('aria-label', texto);
                $('#conn').title = texto;
            }
            const sub = r.ok ? (r.model === 'unavailable' ? 'API disponível' : '') : 'tentando de novo…';
            if ($('#conn-sub').textContent !== sub) $('#conn-sub').textContent = sub;
            if (antes !== online) {
                bus.emit('conn', { ok: r.ok, ms: r.ms });
                if (r.ok && antes === false) carregar();
            }
        } finally { verificando = false; }
    }

    function init() {
        lista = $('#sb-convs-list'); busca = $('#sb-search'); seletor = $('#sb-projeto');
        seletor.addEventListener('change', () => { prefs.set('filtro_projeto', seletor.value); desenhar(); O.anunciar?.(`Conversas: ${seletor.selectedOptions[0].text}.`); });
        bus.on('projetos', () => { montarSeletor(); desenhar(); });
        menu = el('div', { id: 'conv-menu', class: 'conv-menu', role: 'menu', 'aria-label': 'Ações da conversa', hidden: true });
        document.body.append(menu);
        menu.addEventListener('click', e => { const it = e.target.closest('[role="menuitem"]'); if (!it) return; const linha = alvoMenu; fecharMenu(); gerenciar(linha, it.dataset.valor); });
        menu.addEventListener('keydown', e => {
            const itens = [...menu.children], i = itens.indexOf(document.activeElement);
            if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) { e.preventDefault(); const j = e.key === 'Home' ? 0 : e.key === 'End' ? itens.length - 1 : (i + (e.key === 'ArrowDown' ? 1 : -1) + itens.length) % itens.length; itens[j].focus(); }
            else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); fecharMenu(); }
            else if (e.key === 'Tab') fecharMenu(false);
        });
        document.addEventListener('pointerdown', e => { if (!menu.hidden && !menu.contains(e.target) && !alvoMenu.mais.contains(e.target)) fecharMenu(false); });
        $('#sb-toggle').addEventListener('click', alternar);
        $('#sb-new').addEventListener('click', nova);
        maisBusca = el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Mais conversas', on: { click: () => buscarConteudo(geracaoBusca, true) } });
        busca.addEventListener('input', filtrar);
        busca.addEventListener('keydown', e => { if (e.key === 'Enter') { lista.querySelector('.conv')?.focus(); e.preventDefault(); } });
        busca.addEventListener('keydown', e => { if (e.key === 'Escape' && busca.value) { e.stopPropagation(); busca.value = ''; filtrar(); } });
        lista.addEventListener('keydown', e => {
            if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
            const itens = $$('.conv', lista), i = itens.indexOf(document.activeElement);
            if (i < 0) return;
            e.preventDefault();
            itens[Math.max(0, Math.min(itens.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))].focus();
        });
        atualizarToggle();
        const atualizarLista = U.debounce(carregar, 600);
        bus.on('sessoes:atualizar', atualizarLista);
        carregar();
        verificar();
        setInterval(verificar, 8000);
        setInterval(() => { if (!document.hidden) carregar(); }, 20000);   // pega o título automático da 1ª mensagem
        document.addEventListener('visibilitychange', () => { if (!document.hidden) { verificar(); carregar(); } });
    }

    O.sidebar = {
        init, nova, alternar, carregar, abrir, verificar,
        sessoes: () => sessoes, ativa: () => [...(resultadosBusca || []), ...sessoes].find(s => s.sessao_id === (O.historico?.sessao() || ativa)) || null,
        online: () => online, abrindo: () => abrindo,
    };
})();
