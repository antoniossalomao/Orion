/* ==========================================================================
   ORION — views/conhecimento.js | memória da tela e biblioteca de arquivos gerados
   Projetos, fatos e documentos têm tela própria (projects.js, facts.js, documents.js). Aqui ficam
   a memória da tela (regra 44) e o que o Orion gerou (documentos e imagens, com versões).
   Só `textContent`: nada do que vem do servidor vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, icone, api, ui } = O;
    const U = O.util;

    let resultados = [], aberto = null, tela = { ligada: false }, erro = null, relacoes = [], buscaTela = { texto: '', achados: null };
    let pesq = null, rascunho = [], ativa = false, tempoPesq = null;   // pesquisa noturna (E6.5)
    const novoBackend = () => api.estado().backend === 'orion';
    const comPainel = () => novoBackend() && api.suporta('library_panel') && !!O.resultadosPainel;

    const vazio = t => el('p', { class: 'painel-vazio', text: t });
    const data = iso => (iso ? new Date(iso).toLocaleDateString('pt-BR') : '');
    const resumo = (t, n = 140) => (t.length > n ? `${t.slice(0, n)}…` : t);

    async function agir(fn, falha) {
        try { await fn(); await carregar(); return true; }
        catch (e) { ui.toast(`${falha} ${e.message || ''}`.trim(), { tipo: 'erro' }); return false; }
    }

    /* ── resultados: o que o Orion gerou (documentos e imagens), com versões ── */
    const tamanho = n => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);

    async function baixar(r) {
        try {
            const blob = await (await api.bibliotecaArquivo(r.id)).blob();
            const url = URL.createObjectURL(blob);
            const a = el('a', { href: url, download: r.nome });
            document.body.append(a); a.click(); a.remove();
            setTimeout(() => URL.revokeObjectURL(url), 10000);
        } catch (e) { ui.toast(`Não consegui baixar. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
    }

    async function ver(r) {
        if (comPainel()) { O.resultadosPainel.abrir(r.id); return; }   // painel lateral (E6.1)
        if (aberto?.id === r.id) { aberto = null; desenhar(); return; }
        try {
            if (r.previa === 'texto') aberto = { id: r.id, tipo: 'texto', ...(await api.bibliotecaTexto(r.id)) };
            else if (r.previa === 'imagem') aberto = { id: r.id, tipo: 'imagem', url: URL.createObjectURL(await (await api.bibliotecaArquivo(r.id, true)).blob()) };
        } catch (e) { ui.toast(`Não consegui abrir a prévia. ${e.message || ''}`.trim(), { tipo: 'erro' }); return; }
        desenhar();
    }

    async function apagarResultado(r) {
        const ok = await ui.confirmar({
            titulo: 'Apagar este resultado?', ok: 'Apagar', perigo: true,
            texto: `“${r.nome}” (versão ${r.versao}) sai da biblioteca. Outras versões e o arquivo original, se existir, não são tocados.`,
        });
        if (ok && await agir(() => api.bibliotecaApagar(r.id), 'Não consegui apagar.')) { if (aberto?.id === r.id) aberto = null; O.anunciar('Resultado apagado.'); }
    }

    function previaEl(a) {
        if (a.tipo === 'imagem') return el('img', { class: 'conh-previa-img', src: a.url, alt: 'Prévia da imagem gerada' });
        return el('pre', { class: 'conh-previa', text: a.texto + (a.truncado ? '\n…' : '') });
    }

    function itemResultado(r) {
        return el('li', { class: 'painel-item conh-item', dataset: { resultado: String(r.id) } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: r.nome }),
                el('span', { class: 'badge badge-muted', text: `v${r.versao}` }),
                el('span', { class: 'badge badge-muted', text: r.tipo })),
            el('small', { text: `${tamanho(r.bytes)} · ${r.ferramenta}${r.conversa ? ` · conversa “${r.conversa}”` : ''} · ${data(r.criado)}` }),
            aberto?.id === r.id ? previaEl(aberto) : null,
            el('div', { class: 'conh-acoes' },
                r.previa ? el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: aberto?.id === r.id ? 'Fechar prévia' : 'Ver', 'aria-label': `${aberto?.id === r.id ? 'Fechar prévia de' : 'Ver'} ${r.nome} versão ${r.versao}`, on: { click: () => ver(r) } }) : null,
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Baixar', 'aria-label': `Baixar ${r.nome} versão ${r.versao}`, on: { click: () => baixar(r) } }),
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar', 'aria-label': `Apagar ${r.nome} versão ${r.versao}`, on: { click: () => apagarResultado(r) } })));
    }

    /* ── memória da tela (regra 44): só estado e controles; o texto guardado nunca é mostrado ── */
    async function limparTela() {
        const ok = await ui.confirmar({
            titulo: 'Apagar a memória da tela?', ok: 'Apagar tudo', perigo: true,
            texto: 'Todo o texto de tela guardado é apagado do banco e da busca. Isso não desliga a captura.',
        });
        if (ok && await agir(() => api.telaLimpar(), 'Não consegui apagar.')) O.anunciar('Memória da tela apagada.');
    }

    function telaEl() {
        if (!tela.ligada) return vazio('Desligada. Para ligar, use ORION_SCREEN_MEMORY no computador onde o cérebro roda.');
        return el('div', { class: 'conh-tela' },
            el('p', { class: 'painel-top', 'data-tela': '', text: `${tela.pausada ? 'Pausada' : 'Capturando'} · ${tela.registros} registro(s) guardado(s) · ${tela.hoje} nas últimas 24 h · só texto, OCR local` }),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', 'data-tela-pausa': '', text: tela.pausada ? 'Retomar captura' : 'Pausar captura',
                    on: { click: () => agir(() => api.telaPausa(!!tela.pausada), 'Não consegui mudar.') } }),
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar tudo', on: { click: limparTela } })),
            api.estado().backend === 'orion' && api.suporta('screen_search') ? buscaDaTelaEl() : null);
    }

    /* ── busca no texto da tela (E5.1): quando, em qual janela/programa, e o trecho ── */
    const quando = ts => new Date(ts * 1000).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
    async function procurarNaTela(texto) {
        buscaTela.texto = texto;
        if (texto.trim().length < 2) { buscaTela.achados = null; desenhar(); return; }
        try { buscaTela.achados = (await api.telaBusca(texto.trim(), 30)).resultados; }
        catch (e) { buscaTela.achados = null; ui.toast(`Não consegui buscar na tela. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
        desenhar();
        $('#tela-busca-campo')?.focus();
    }
    function buscaDaTelaEl() {
        const campo = el('input', { id: 'tela-busca-campo', class: 'input', type: 'search', 'aria-label': 'Buscar no texto da tela', placeholder: 'Buscar no que esteve na tela…', value: buscaTela.texto });
        campo.addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); procurarNaTela(campo.value); } });
        const lista = buscaTela.achados == null ? null : buscaTela.achados.length
            ? el('ul', { class: 'painel-lista', 'aria-label': 'Resultados na memória da tela' }, ...buscaTela.achados.map(a => el('li', { class: 'painel-item', dataset: { telaResultado: String(a.id) } },
                el('small', { text: `${quando(a.quando)} · ${a.janela || 'sem título'}${a.app ? ` · ${a.app}` : ''}` }),
                el('p', { class: 'selectable', text: a.trecho }))))
            : vazio('Nada na memória da tela com esse texto.');
        return el('div', { class: 'conh-tela-busca' }, campo, lista);
    }

    /* ── relações do ciclo de sono (E5.4): o que o Orion ligou entre as coisas; dá para apagar ── */
    async function apagarRelacao(r) {
        const ok = await ui.confirmar({ titulo: 'Apagar esta relação?', ok: 'Apagar', perigo: true,
            texto: `“${r.src}” —${r.rel}→ “${r.dst}” sai do grafo. Os fatos continuam como estão.` });
        if (ok && await agir(() => api.apagarRelacaoSono(r.src, r.rel, r.dst), 'Não consegui apagar.')) O.anunciar('Relação apagada.');
    }
    function relacoesEl() {
        if (!relacoes.length) return vazio('Nenhuma relação ainda. Com ORION_SLEEP_AT ligado, o Orion liga as coisas citadas nos seus fatos toda noite.');
        return el('ul', { class: 'painel-lista', 'aria-label': 'Relações do ciclo de sono' }, ...relacoes.map(r => el('li', { class: 'painel-item', dataset: { relacao: `${r.src}|${r.rel}|${r.dst}` } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: r.src }), el('span', { class: 'badge badge-muted', text: r.rel }), el('strong', { text: r.dst })),
            el('div', { class: 'conh-acoes' }, el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar', 'aria-label': `Apagar a relação ${r.src} ${r.rel} ${r.dst}`, on: { click: () => apagarRelacao(r) } })))));
    }

    /* ── pesquisa noturna (E6.5, regra 39): assuntos editáveis, relatórios e "rodar agora" ── */
    const MB = n => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);
    const mudou = () => JSON.stringify(rascunho.map(x => x.trim()).filter(Boolean)) !== JSON.stringify(pesq.assuntos);

    async function carregarPesquisa() {
        if (!novoBackend() || !api.suporta('research_ui')) { pesq = null; return; }
        try {
            const [a, r] = await Promise.all([api.pesquisaAssuntos(), api.pesquisaRelatorios()]);
            const primeira = pesq === null;
            pesq = { ...a, relatorios: r.relatorios || [] };
            if (primeira || !mudou()) rascunho = [...pesq.assuntos];
        } catch (e) { pesq = null; }
        armarPesquisa();
    }
    function armarPesquisa() {
        clearTimeout(tempoPesq);
        if (ativa && pesq?.rodando) tempoPesq = setTimeout(async () => { await carregarPesquisa(); if (ativa) desenhar(); }, 4000);
    }
    async function salvarAssuntos() {
        const prep = O.resultadosLogica.prepararAssuntos(rascunho, pesq.max, pesq.max_tamanho);
        if (prep.erro) { ui.toast(prep.erro, { tipo: 'aviso' }); return; }
        try {
            const r = await api.pesquisaSalvar(prep.itens);
            pesq = { ...pesq, ...r }; rascunho = [...pesq.assuntos];
            O.anunciar('Assuntos da pesquisa salvos.'); desenhar();
        } catch (e) { ui.toast(`Não salvei os assuntos. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
    }
    async function rodarPesquisa() {
        try {
            await api.pesquisaRodar();
            pesq = { ...pesq, rodando: true };
            ui.toast('Pesquisa iniciada. O relatório cai no 00 Inbox do vault.', { tipo: 'ok' });
            armarPesquisa(); desenhar();
        } catch (e) { ui.toast(e.message || 'Não consegui iniciar a pesquisa.', { tipo: 'erro' }); }
    }
    function pesquisaEl() {
        const max = pesq.max || 5;
        const linhas = rascunho.map((texto, i) => el('li', { class: 'pesq-assunto' },
            el('input', { class: 'input', type: 'text', value: texto, maxlength: String(pesq.max_tamanho || 200), 'aria-label': `Assunto ${i + 1}`, placeholder: 'Ex.: MCP (Model Context Protocol) segurança',
                on: { input: e => { rascunho[i] = e.target.value; const b = $('#pesq-salvar'); if (b) b.disabled = !mudou(); } } }),
            el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Remover', 'aria-label': `Remover o assunto ${i + 1}`,
                on: { click: () => { rascunho.splice(i, 1); desenhar(); } } })));
        return el('div', { class: 'conh-tela', dataset: { pesquisa: '' } },
            el('p', { class: 'painel-top', text: `${pesq.hora ? `Roda todo dia às ${pesq.hora}` : 'Sem hora marcada: só roda quando você mandar'} · só leitura · até ${max} assuntos${pesq.origem === 'padrao' ? ' · usando os do .env' : ''}` }),
            rascunho.length ? el('ul', { class: 'pesq-assuntos', 'aria-label': 'Assuntos da pesquisa' }, ...linhas) : vazio('Nenhum assunto. Adicione ao menos um para pesquisar.'),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Adicionar assunto', disabled: rascunho.length >= max, on: { click: () => { rascunho.push(''); desenhar(); $$('.pesq-assunto input').at(-1)?.focus(); } } }),
                el('button', { id: 'pesq-salvar', class: 'btn btn-primary btn-sm', type: 'button', text: 'Salvar assuntos', disabled: !mudou(), on: { click: salvarAssuntos } }),
                el('button', { id: 'pesq-rodar', class: 'btn btn-outline btn-sm', type: 'button', text: pesq.rodando ? 'Pesquisando…' : 'Rodar agora', disabled: !pesq.disponivel || pesq.rodando || mudou(), on: { click: rodarPesquisa } })),
            !pesq.disponivel ? vazio('Indisponível neste computador: precisa de ORION_WEB_TOOLS, de um modelo no gateway e do vault.') : mudou() ? vazio('Salve os assuntos para poder rodar agora.') : null,
            pesq.relatorios.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Relatórios da pesquisa' }, ...pesq.relatorios.map(r => el('li', { class: 'painel-item', dataset: { relatorio: r.arquivo } },
                el('strong', { text: r.arquivo.split('/').pop() }), el('small', { text: `${O.resultadosLogica.dataBR(r.data)} · ${MB(r.bytes)} · ${r.arquivo}` })))) : vazio('Nenhum relatório ainda no 00 Inbox.'));
    }

    function cartao(id, titulo, dica, ...filhos) {
        return el('section', { class: 'card painel-card', 'aria-labelledby': `cn-${id}`, dataset: { id } },
            el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: `cn-${id}`, text: titulo }),
                dica ? el('span', { class: 'card-sub', text: dica }) : null),
            ...filhos);
    }

    function desenhar() {
        const raiz = $('#conhecimento-corpo');
        raiz.setAttribute('aria-busy', 'false');
        if (erro) {
            raiz.replaceChildren(el('div', { class: 'banner banner-warn', role: 'status' }, el('span', { html: icone('alert') }),
                el('div', {}, el('strong', { text: 'O cérebro não respondeu. ' }), 'Confira a conexão em Configurações e atualize.')));
            return;
        }
        raiz.replaceChildren(el('div', { class: 'grid grid-2 painel-grade' },
            cartao('tela', 'Memória da tela', tela.ligada ? 'ligada' : 'desligada', telaEl()),
            api.estado().backend === 'orion' && api.suporta('memory_graph') ? cartao('relacoes', 'Relações do sono', `${relacoes.length} relação(ões)`, relacoesEl()) : null,
            pesq ? cartao('pesquisa', 'Pesquisa noturna', pesq.rodando ? 'rodando…' : `${pesq.assuntos.length} assunto(s)`, pesquisaEl()) : null,
            cartao('resultados', 'Arquivos gerados', `${resultados.length} gerado(s)`,
                comPainel() && resultados.length ? el('div', { class: 'conh-acoes' }, el('button', { class: 'btn btn-outline btn-sm', type: 'button', id: 'res-limpar', text: 'Limpar versões antigas', on: { click: () => O.resultadosPainel.limparAntigas() } })) : null,
                resultados.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Arquivos gerados' }, ...resultados.map(itemResultado))
                    : vazio('Nada gerado ainda. Documentos e imagens que o Orion criar aparecem aqui, com versões.'))));
    }

    async function carregar() {
        try {
            const [t, rs, rel] = await Promise.all([api.tela().catch(() => ({ ligada: false })), api.biblioteca().catch(() => ({ resultados: [] })),
                api.estado().backend === 'orion' && api.suporta('memory_graph') ? api.relacoesSono().catch(() => []) : []]);
            resultados = rs.resultados || []; tela = t || { ligada: false }; relacoes = Array.isArray(rel) ? rel : []; erro = null;
            await carregarPesquisa();
        } catch (e) { erro = e; }
        desenhar();
    }

    O.views = O.views || {};
    O.views.conhecimento = {
        init() { $('#conhecimento-refresh').addEventListener('click', carregar); O.bus.on('resultados:mudou', () => { if (ativa) carregar(); }); O.conhecimento = { buscarNaTela: t => { buscaTela.texto = t; O.app.ir('conhecimento'); return procurarNaTela(t); } }; },
        ativar() { ativa = true; carregar(); },
        desativar() { ativa = false; clearTimeout(tempoPesq); },
    };
})();
