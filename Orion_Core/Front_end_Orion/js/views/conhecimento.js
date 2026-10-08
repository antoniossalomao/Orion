/* ==========================================================================
   ORION — views/conhecimento.js | projetos (instruções por grupo de conversas) e fatos da memória
   Tudo vem de /projetos e /memoria/fatos (orion/app.py). Só `textContent`: texto de fato ou de
   projeto nunca vira HTML. Esquecer um fato apaga de verdade (texto, índice e vetor).
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, api, ui } = O;
    const U = O.util;

    let projetos = [], fatos = [], documentos = [], resultados = [], plugins = [], aberto = null, enviando = false, tela = { ligada: false }, filtro = '', erro = null, tokenBusca = 0;

    const vazio = t => el('p', { class: 'painel-vazio', text: t });
    const data = iso => (iso ? new Date(iso).toLocaleDateString('pt-BR') : '');
    const resumo = (t, n = 140) => (t.length > n ? `${t.slice(0, n)}…` : t);

    async function agir(fn, falha) {
        try { await fn(); await carregar(); return true; }
        catch (e) { ui.toast(`${falha} ${e.message || ''}`.trim(), { tipo: 'erro' }); return false; }
    }

    /* ── projetos ──────────────────────────────────────────────────────── */
    function formProjeto() {
        const nome = el('input', { id: 'proj-nome', class: 'input', type: 'text', maxlength: '80', autocomplete: 'off' });
        const instr = el('textarea', { id: 'proj-instr', class: 'input', rows: '3', maxlength: '4000' });
        const form = el('form', { class: 'conh-form', novalidate: true },
            el('div', { class: 'field' }, el('label', { for: 'proj-nome', text: 'Nome do projeto' }), nome),
            el('div', { class: 'field' }, el('label', { for: 'proj-instr', text: 'Instruções (entram no prompt das conversas do projeto)' }), instr),
            el('button', { class: 'btn btn-primary btn-sm', type: 'submit', text: 'Criar projeto' }));
        form.addEventListener('submit', async e => {
            e.preventDefault();
            const n = nome.value.replace(/\s+/g, ' ').trim();
            if (!n) { nome.focus(); return; }
            if (await agir(() => api.criarProjeto(n, instr.value.trim()), 'Não consegui criar o projeto.')) {
                nome.value = ''; instr.value = ''; O.anunciar(`Projeto ${n} criado.`);
            }
        });
        return form;
    }

    async function editarInstrucoes(p) {
        const novo = await ui.perguntar({ titulo: `Instruções de ${p.nome}`, rotulo: 'Instruções', valor: p.instrucoes, ok: 'Salvar', max: 4000, multilinha: true });
        if (novo == null || novo === p.instrucoes) return;
        if (await agir(() => api.ajustarProjeto(p.id, { instrucoes: novo }), 'Não consegui salvar.')) O.anunciar('Instruções salvas.');
    }

    async function apagarProjeto(p) {
        const ok = await ui.confirmar({
            titulo: 'Apagar este projeto?', ok: 'Apagar', perigo: true,
            texto: `“${p.nome}” some. As conversas dele continuam, só deixam de ter essas instruções.`,
        });
        if (ok && await agir(() => api.apagarProjeto(p.id), 'Não consegui apagar.')) O.anunciar('Projeto apagado.');
    }

    function itemProjeto(p) {
        return el('li', { class: 'painel-item conh-item', dataset: { projeto: String(p.id) } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: p.nome }),
                p.arquivado ? el('span', { class: 'badge badge-muted', text: 'Arquivado' }) : null),
            el('small', { text: p.instrucoes ? resumo(p.instrucoes, 220) : 'Sem instruções.' }),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Editar instruções', 'aria-label': `Editar instruções de ${p.nome}`, on: { click: () => editarInstrucoes(p) } }),
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: p.arquivado ? 'Desarquivar' : 'Arquivar', 'aria-label': `${p.arquivado ? 'Desarquivar' : 'Arquivar'} ${p.nome}`,
                    on: { click: () => agir(() => api.ajustarProjeto(p.id, { arquivado: !p.arquivado }), 'Não consegui mudar.') } }),
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar', 'aria-label': `Apagar projeto ${p.nome}`, on: { click: () => apagarProjeto(p) } })));
    }

    /* ── documentos (PDF, Word, Excel, HTML, texto) ─────────────────────── */
    async function enviarDoc(arquivo, projetoId) {
        enviando = true; desenhar();
        try {
            const r = await api.enviarDocumento(arquivo, projetoId);
            O.anunciar(r.resultado === 'same' ? 'Esse documento já estava na memória.' : `Documento indexado em ${r.trechos} trecho(s).`);
            ui.toast(r.resultado === 'same' ? 'Esse documento já estava na memória.' : `“${arquivo.name}” entrou na memória (${r.trechos} trecho(s)).`, { tipo: 'ok' });
        } catch (e) { ui.toast(`Não consegui indexar. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
        enviando = false;
        await carregar();
    }

    function formDocumento() {
        const arq = el('input', { id: 'doc-arquivo', class: 'input', type: 'file', accept: '.pdf,.docx,.xlsx,.txt,.md,.csv,.json,.html,.htm' });
        const proj = el('select', { id: 'doc-projeto', class: 'input' },
            el('option', { value: '', text: '(vale para todas as conversas)' }),
            ...projetos.filter(p => !p.arquivado).map(p => el('option', { value: String(p.id), text: p.nome })));
        const botao = el('button', { class: 'btn btn-primary btn-sm', type: 'submit', text: enviando ? 'Indexando…' : 'Enviar para a memória' });
        botao.disabled = enviando;
        const form = el('form', { class: 'conh-form', novalidate: true },
            el('div', { class: 'field' }, el('label', { for: 'doc-arquivo', text: 'Documento' }), arq),
            el('div', { class: 'field' }, el('label', { for: 'doc-projeto', text: 'Disponível em' }), proj), botao);
        form.addEventListener('submit', e => {
            e.preventDefault();
            if (!arq.files?.length) { arq.focus(); return; }
            enviarDoc(arq.files[0], proj.value);
        });
        return form;
    }

    async function apagarDoc(d) {
        const ok = await ui.confirmar({
            titulo: 'Remover este documento?', ok: 'Remover', perigo: true,
            texto: `“${d.nome}” sai da busca da memória. O arquivo original, no seu computador, não é tocado.`,
        });
        if (ok && await agir(() => api.apagarDocumento(d.id), 'Não consegui remover.')) O.anunciar('Documento removido.');
    }

    function itemDocumento(d) {
        const proj = projetos.find(p => p.id === d.projeto_id);
        return el('li', { class: 'painel-item conh-item', dataset: { documento: String(d.id) } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: d.nome }),
                el('span', { class: 'badge badge-muted', text: proj ? `Projeto: ${proj.nome}` : 'Todas as conversas' })),
            el('small', { text: `${d.trechos} trecho(s) · indexado em ${data(d.indexado)}` }),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Remover', 'aria-label': `Remover documento ${d.nome}`, on: { click: () => apagarDoc(d) } })));
    }

    /* ── plugins: instalar é pela linha de comando; aqui você lê o que ele libera e concede ou revoga ── */
    const ESTADO_PLUGIN = { ativo: ['Ativo', 'ok'], mudou: ['Mudou desde a concessão', 'warn'], sem_concessao: ['Sem concessão', 'muted'] };

    async function conceder(p) {
        const mcp = p.servidores.map(s => `${s.nome} (${s.transporte}: ${s.executa})`).join('; ') || 'nenhum servidor';
        const ok = await ui.confirmar({
            titulo: `Conceder o plugin ${p.nome}?`, ok: 'Conceder',
            texto: `Libera ${p.skills} skill(s) e estes servidores MCP: ${mcp}. Cada ferramenta segue a classe de risco do plugin e as aprovações de sempre. Vale depois de reiniciar o Orion; qualquer mudança no pacote cancela a concessão.`,
        });
        if (ok && await agir(() => api.concederPlugin(p.nome), 'Não consegui conceder.')) O.anunciar('Plugin concedido. Reinicie o Orion para valer.');
    }

    function itemPlugin(p) {
        const [rotulo, nivel] = ESTADO_PLUGIN[p.estado] || [p.estado, 'muted'];
        return el('li', { class: 'painel-item conh-item', dataset: { plugin: p.nome, estado: p.estado } },
            el('div', { class: 'painel-item-top' }, el('strong', { text: p.nome }), el('span', { class: 'mono', text: p.versao }),
                el('span', { class: `badge badge-${nivel}`, text: rotulo })),
            el('small', { text: p.descricao }),
            el('small', { text: `${p.skills} skill(s) · ${p.servidores.length} servidor(es) MCP${p.servidores.length ? ': ' + p.servidores.map(s => s.nome).join(', ') : ''}` }),
            el('div', { class: 'conh-acoes' },
                p.estado === 'ativo'
                    ? el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Revogar', 'aria-label': `Revogar o plugin ${p.nome}`,
                        on: { click: () => agir(() => api.revogarPlugin(p.nome), 'Não consegui revogar.') } })
                    : el('button', { class: 'btn btn-primary btn-sm', type: 'button', text: p.estado === 'mudou' ? 'Conceder de novo' : 'Conceder', 'aria-label': `Conceder o plugin ${p.nome}`,
                        on: { click: () => conceder(p) } })));
    }

    /* ── resultados: o que o Orion gerou (documentos e imagens), com versões ── */
    const tamanho = n => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);

    async function baixar(r) {
        try {
            const blob = await (await api.arquivoDoResultado(r.id)).blob();
            const url = URL.createObjectURL(blob);
            const a = el('a', { href: url, download: r.nome });
            document.body.append(a); a.click(); a.remove();
            setTimeout(() => URL.revokeObjectURL(url), 10000);
        } catch (e) { ui.toast(`Não consegui baixar. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
    }

    async function ver(r) {
        if (aberto?.id === r.id) { aberto = null; desenhar(); return; }
        try {
            if (r.previa === 'texto') aberto = { id: r.id, tipo: 'texto', ...(await api.textoDoResultado(r.id)) };
            else if (r.previa === 'imagem') aberto = { id: r.id, tipo: 'imagem', url: URL.createObjectURL(await (await api.arquivoDoResultado(r.id, true)).blob()) };
        } catch (e) { ui.toast(`Não consegui abrir a prévia. ${e.message || ''}`.trim(), { tipo: 'erro' }); return; }
        desenhar();
    }

    async function apagarResultado(r) {
        const ok = await ui.confirmar({
            titulo: 'Apagar este resultado?', ok: 'Apagar', perigo: true,
            texto: `“${r.nome}” (versão ${r.versao}) sai da biblioteca. Outras versões e o arquivo original, se existir, não são tocados.`,
        });
        if (ok && await agir(() => api.apagarResultado(r.id), 'Não consegui apagar.')) { if (aberto?.id === r.id) aberto = null; O.anunciar('Resultado apagado.'); }
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

    /* ── fatos ─────────────────────────────────────────────────────────── */
    async function corrigir(f) {
        const novo = await ui.perguntar({ titulo: 'Corrigir fato', rotulo: 'O que o Orion deve saber', valor: f.texto, ok: 'Salvar', max: 2000 });
        if (novo == null || novo === f.texto) return;
        if (await agir(() => api.corrigirFato(f.id, novo), 'Não consegui corrigir.')) O.anunciar('Fato corrigido.');
    }

    async function esquecer(f) {
        const ok = await ui.confirmar({
            titulo: 'Esquecer este fato?', ok: 'Esquecer', perigo: true,
            texto: `“${resumo(f.texto, 120)}” é apagado do banco e da busca. Backups antigos e conversas que repetem o fato não são tocados.`,
        });
        if (ok && await agir(() => api.esquecerFato(f.id), 'Não consegui esquecer.')) O.anunciar('Fato esquecido.');
    }

    function itemFato(f) {
        return el('li', { class: 'painel-item conh-item', dataset: { fato: String(f.id) } },
            el('div', { class: 'conh-fato', text: f.texto }),
            el('small', { text: `fonte: ${f.fonte} · atualizado em ${data(f.atualizado)}` }),
            el('div', { class: 'conh-acoes' },
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Corrigir', 'aria-label': `Corrigir fato: ${resumo(f.texto, 40)}`, on: { click: () => corrigir(f) } }),
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Esquecer', 'aria-label': `Esquecer fato: ${resumo(f.texto, 40)}`, on: { click: () => esquecer(f) } })));
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
                el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: 'Apagar tudo', on: { click: limparTela } })));
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
        const busca = el('input', { id: 'fatos-busca', class: 'input', type: 'search', maxlength: '200', autocomplete: 'off', value: filtro });
        busca.addEventListener('input', () => { filtro = busca.value.trim(); buscar(); });
        raiz.replaceChildren(el('div', { class: 'grid grid-2 painel-grade' },
            cartao('tela', 'Memória da tela', tela.ligada ? 'ligada' : 'desligada', telaEl()),
            cartao('projetos', 'Projetos', `${projetos.length} ativo(s)`, formProjeto(),
                projetos.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Projetos' }, ...projetos.map(itemProjeto))
                    : vazio('Nenhum projeto ainda. Crie um para dar instruções próprias a um grupo de conversas.')),
            cartao('documentos', 'Documentos', `${documentos.length} na memória`, formDocumento(),
                documentos.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Documentos' }, ...documentos.map(itemDocumento))
                    : vazio('Nenhum documento enviado. PDF, Word, Excel, HTML e texto viram trechos pesquisáveis.')),
            cartao('plugins', 'Plugins', 'valem depois de reiniciar',
                plugins.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Plugins' }, ...plugins.map(itemPlugin))
                    : vazio('Nenhum plugin instalado. Instale uma pasta com “orion plugin instalar <pasta>”; nada vale sem a sua concessão.')),
            cartao('resultados', 'Resultados', `${resultados.length} gerado(s)`,
                resultados.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Resultados' }, ...resultados.map(itemResultado))
                    : vazio('Nada gerado ainda. Documentos e imagens que o Orion criar aparecem aqui, com versões.')),
            cartao('fatos', 'O que o Orion sabe sobre você', `${fatos.length} fato(s)`,
                el('div', { class: 'field' }, el('label', { for: 'fatos-busca', text: 'Buscar nos fatos' }), busca),
                fatos.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Fatos' }, ...fatos.map(itemFato))
                    : vazio(filtro ? `Nenhum fato casa com “${filtro}”.` : 'Ainda não há fatos guardados.'))));
        if (filtro) { const b = $('#fatos-busca'); b.focus(); b.setSelectionRange(b.value.length, b.value.length); }
    }

    const buscar = U.debounce(async () => {
        const meu = ++tokenBusca, consulta = filtro;
        try {
            const d = await api.fatos(consulta);
            if (meu !== tokenBusca || consulta !== filtro) return;
            fatos = d.fatos || [];
            desenhar();
        } catch (_) { /* a lista anterior continua valendo */ }
    }, 300);

    async function carregar() {
        try {
            const [p, f, t, dc, rs, pl] = await Promise.all([api.projetos(), api.fatos(filtro), api.tela().catch(() => ({ ligada: false })),
                api.documentos().catch(() => ({ documentos: [] })), api.resultados().catch(() => ({ resultados: [] })), api.plugins().catch(() => ({ plugins: [] }))]);
            projetos = p.projetos || []; fatos = f.fatos || []; documentos = dc.documentos || []; resultados = rs.resultados || []; plugins = pl.plugins || []; tela = t || { ligada: false }; erro = null;
        } catch (e) { erro = e; }
        desenhar();
    }

    O.views = O.views || {};
    O.views.conhecimento = {
        init() { $('#conhecimento-refresh').addEventListener('click', carregar); },
        ativar() { carregar(); },
        desativar() {},
    };
})();
