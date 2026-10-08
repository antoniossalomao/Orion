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

    let projetos = [], fatos = [], filtro = '', erro = null, tokenBusca = 0;

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
        const novo = await ui.perguntar({ titulo: `Instruções de ${p.nome}`, rotulo: 'Instruções', valor: p.instrucoes, ok: 'Salvar', max: 4000 });
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
            cartao('projetos', 'Projetos', `${projetos.length} ativo(s)`, formProjeto(),
                projetos.length ? el('ul', { class: 'painel-lista', 'aria-label': 'Projetos' }, ...projetos.map(itemProjeto))
                    : vazio('Nenhum projeto ainda. Crie um para dar instruções próprias a um grupo de conversas.')),
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
            const [p, f] = await Promise.all([api.projetos(), api.fatos(filtro)]);
            projetos = p.projetos || []; fatos = f.fatos || []; erro = null;
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
