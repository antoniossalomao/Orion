/* ==========================================================================
   ORION — views/conhecimento.js | memória da tela e biblioteca de arquivos gerados
   Projetos, fatos e documentos têm tela própria (projects.js, facts.js, documents.js). Aqui ficam
   a memória da tela (regra 44) e o que o Orion gerou (documentos e imagens, com versões).
   Só `textContent`: nada do que vem do servidor vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, api, ui } = O;
    const U = O.util;

    let resultados = [], aberto = null, tela = { ligada: false }, erro = null;

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
        raiz.replaceChildren(el('div', { class: 'grid grid-2 painel-grade' },
            cartao('tela', 'Memória da tela', tela.ligada ? 'ligada' : 'desligada', telaEl()),
            cartao('resultados', 'Arquivos gerados', `${resultados.length} gerado(s)`,
                resultados.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Arquivos gerados' }, ...resultados.map(itemResultado))
                    : vazio('Nada gerado ainda. Documentos e imagens que o Orion criar aparecem aqui, com versões.'))));
    }

    async function carregar() {
        try {
            const [t, rs] = await Promise.all([api.tela().catch(() => ({ ligada: false })), api.biblioteca().catch(() => ({ resultados: [] }))]);
            resultados = rs.resultados || []; tela = t || { ligada: false }; erro = null;
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
