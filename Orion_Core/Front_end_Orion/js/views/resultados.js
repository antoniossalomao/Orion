/* ==========================================================================
   ORION — views/resultados.js | painel lateral da biblioteca (E6.1 a E6.3)
   Abre ao lado da tela atual: prévia de texto, imagem e PDF, as versões do mesmo arquivo e a
   comparação entre elas. Só `textContent`; imagem e PDF vão por URL de blob (a CSP só deixa
   `frame-src blob:`), então nada do que o servidor manda vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, api, bus, ui } = O;

    let aberto = null, geracao = 0, urls = [], versoes = [];
    const tamanho = n => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);
    const data = iso => (iso ? new Date(iso).toLocaleDateString('pt-BR') : '');   // ISO com hora: o fuso é o seu
    const dica = texto => el('p', { class: 'extension-hint', text: texto });
    const botao = (texto, fn, cls = 'btn btn-outline btn-sm', rotulo = null) => el('button', { type: 'button', class: cls, text: texto, ...(rotulo ? { 'aria-label': rotulo } : {}), on: { click: fn } });
    const raiz = () => $('#res-painel');

    function soltar() { urls.forEach(u => URL.revokeObjectURL(u)); urls = []; }
    function blobUrl(blob) { const u = URL.createObjectURL(blob); urls.push(u); return u; }

    function fechar(foco = true) {
        ++geracao; soltar();
        const id = aberto; aberto = null; versoes = [];
        const p = raiz(); if (!p) return;
        p.hidden = true; p.replaceChildren();
        document.documentElement.dataset.resAberto = 'false';
        if (foco && id) document.querySelector(`[data-resultado="${CSS.escape(String(id))}"] button`)?.focus();
    }

    async function baixar(r) {
        try {
            const blob = await (await api.bibliotecaArquivo(r.id)).blob();
            const a = el('a', { href: blobUrl(blob), download: r.nome });
            document.body.append(a); a.click(); a.remove();
        } catch (e) { ui.toast(`Não consegui baixar. ${e.message || ''}`.trim(), { tipo: 'erro' }); }
    }

    async function apagar(r) {
        const ok = await ui.confirmar({ titulo: 'Apagar este resultado?', ok: 'Apagar', perigo: true,
            texto: `“${r.nome}” (versão ${r.versao}) sai da biblioteca. As outras versões não são tocadas.` });
        if (!ok) return;
        try { await api.bibliotecaApagar(r.id); } catch (e) { ui.toast(`Não consegui apagar. ${e.message || ''}`.trim(), { tipo: 'erro' }); return; }
        O.anunciar?.('Resultado apagado.');
        fechar(false); bus.emit('resultados:mudou');
    }

    async function previa(r, token) {
        const caixa = el('div', { class: 'res-previa', dataset: { previa: r.previa || 'nenhuma' } });
        if (r.previa === 'texto') {
            const t = await api.bibliotecaTexto(r.id);
            caixa.append(el('pre', { class: 'res-texto selectable', tabindex: '0', 'aria-label': `Conteúdo de ${r.nome}`, text: t.texto + (t.truncado ? '\n…' : '') }));
        } else if (r.previa === 'imagem') {
            const blob = await (await api.bibliotecaArquivo(r.id, true)).blob();
            caixa.append(el('img', { class: 'res-img', src: blobUrl(blob), alt: `Prévia de ${r.nome}` }));
        } else if (r.previa === 'pdf') {
            const blob = await (await api.bibliotecaArquivo(r.id, true)).blob();
            caixa.append(el('iframe', { class: 'res-pdf', title: `PDF ${r.nome}`, src: blobUrl(new Blob([blob], { type: 'application/pdf' })) }));
        } else caixa.append(dica('Sem prévia para este tipo de arquivo. Baixe para abrir.'));
        return token === geracao ? caixa : null;
    }

    /* ── comparar versões (E6.2) ── */
    function linhaDiff(l) {
        const tipo = O.resultadosLogica.tipoLinhaDiff(l);
        return el('span', { class: `res-diff-l res-diff-${tipo}`, dataset: { diff: tipo }, text: l || ' ' });
    }

    async function comparar(atual, outra, destino, token) {
        destino.replaceChildren(dica('Comparando…'));
        try {
            if (atual.previa === 'imagem' && outra.previa === 'imagem') {
                const [a, b] = await Promise.all([outra, atual].map(async r => blobUrl(await (await api.bibliotecaArquivo(r.id, true)).blob())));
                if (token !== geracao) return;
                destino.replaceChildren(el('div', { class: 'res-lado', dataset: { comparacao: 'imagens' } },
                    el('figure', {}, el('img', { class: 'res-img', src: a, alt: `Versão ${outra.versao}` }), el('figcaption', { text: `Versão ${outra.versao}` })),
                    el('figure', {}, el('img', { class: 'res-img', src: b, alt: `Versão ${atual.versao}` }), el('figcaption', { text: `Versão ${atual.versao}` }))));
                return;
            }
            if (atual.previa !== 'texto' || outra.previa !== 'texto') { destino.replaceChildren(dica('Só texto, Markdown, CSV e imagem podem ser comparados.')); return; }
            const d = await api.bibliotecaDiff(atual.id, outra.id);
            if (token !== geracao) return;
            if (d.iguais) { destino.replaceChildren(dica(`As versões ${outra.versao} e ${atual.versao} são iguais.`)); return; }
            destino.replaceChildren(
                el('p', { class: 'res-diff-resumo', role: 'status', text: `v${outra.versao} → v${atual.versao}: +${d.adicionadas} −${d.removidas}${d.truncado ? ' (diff abreviado)' : ''}` }),
                el('pre', { class: 'res-diff selectable', tabindex: '0', 'aria-label': `Diferenças entre as versões ${outra.versao} e ${atual.versao}` }, ...d.linhas.map(linhaDiff)));
        } catch (e) { if (token === geracao) destino.replaceChildren(dica(`Não consegui comparar. ${e.message || ''}`.trim())); }
    }

    function versoesEl(r, token) {
        const outras = versoes.filter(v => v.id !== r.id);
        const alvo = el('div', { class: 'res-comparacao', 'aria-live': 'polite' });
        if (!outras.length) return el('section', { class: 'res-versoes' }, el('h4', { text: 'Versões' }), dica('Só há esta versão.'));
        const escolha = el('select', { class: 'input', id: 'res-comparar', 'aria-label': 'Comparar com a versão' },
            el('option', { value: '', text: 'Comparar com…' }),
            ...outras.map(v => el('option', { value: String(v.id), text: `v${v.versao} · ${data(v.criado)}` })));
        escolha.addEventListener('change', () => {
            const v = outras.find(x => String(x.id) === escolha.value);
            if (v) comparar(r, v, alvo, token); else alvo.replaceChildren();
        });
        return el('section', { class: 'res-versoes' }, el('h4', { text: 'Versões' }),
            el('ul', { class: 'res-lista-versoes', 'aria-label': 'Versões deste arquivo' }, ...versoes.map(v => el('li', {},
                v.id === r.id ? el('strong', { text: `v${v.versao} · esta` }) : botao(`v${v.versao} · ${data(v.criado)}`, () => abrir(v.id), 'btn btn-ghost btn-sm', `Abrir a versão ${v.versao} de ${v.nome}`)))),
            escolha, alvo);
    }

    async function abrir(id) {
        const p = raiz(); if (!p) return;
        const token = ++geracao; soltar(); aberto = id;
        p.hidden = false; document.documentElement.dataset.resAberto = 'true';
        p.replaceChildren(dica('Abrindo…'));
        try {
            const todos = (await api.biblioteca()).resultados || [];
            const r = todos.find(x => x.id === id);
            if (!r) throw new Error('Esse resultado não existe mais.');
            versoes = todos.filter(x => x.nome === r.nome && x.tipo === r.tipo).sort((a, b) => b.versao - a.versao);
            const caixa = await previa(r, token);
            if (!caixa) return;
            const titulo = el('h3', { tabindex: '-1', text: r.nome });
            p.replaceChildren(
                el('div', { class: 'res-cab' }, titulo, el('span', { class: 'badge badge-muted', text: `v${r.versao}` }), botao('Fechar', () => fechar(), 'btn btn-ghost btn-sm', 'Fechar o painel do resultado')),
                el('small', { class: 'res-meta', text: `${tamanho(r.bytes)} · ${r.ferramenta}${r.conversa ? ` · conversa “${r.conversa}”` : ''} · ${data(r.criado)}` }),
                el('div', { class: 'conh-acoes' }, botao('Baixar', () => baixar(r), 'btn btn-outline btn-sm', `Baixar ${r.nome} versão ${r.versao}`), botao('Apagar', () => apagar(r), 'btn btn-ghost btn-sm', `Apagar ${r.nome} versão ${r.versao} do painel`)),
                caixa, versoesEl(r, token));
            titulo.focus({ preventScroll: true });
        } catch (e) {
            if (token === geracao) p.replaceChildren(dica(e.message || 'Não consegui abrir.'), botao('Fechar', () => fechar(false), 'btn btn-ghost btn-sm'));
        }
    }

    /* ── limpar versões antigas em lote (E6.3): o total aparece antes de apagar ── */
    async function limparAntigas() {
        const texto = await ui.perguntar({ titulo: 'Limpar versões antigas', rotulo: 'Quantas versões manter de cada arquivo (as mais novas)', valor: '3', ok: 'Ver o que sai', max: 4 });
        if (texto === null) return false;
        const manter = Number.parseInt(texto, 10);
        if (!Number.isInteger(manter) || manter < 1 || manter > 1000) { ui.toast('Informe um número de 1 a 1000.', { tipo: 'aviso' }); return false; }
        try {
            const sim = await api.bibliotecaLimpar(manter, true);
            if (!sim.total) { ui.toast(`Nada a limpar: nenhum arquivo tem mais de ${manter} versão(ões).`, { tipo: 'info' }); return false; }
            const ok = await ui.confirmar({ titulo: `Apagar ${sim.total} versão(ões) antiga(s)?`, ok: 'Apagar', perigo: true,
                texto: `Ficam as ${manter} mais novas de cada arquivo. Saem ${sim.total} versão(ões), ${tamanho(sim.bytes)} no total. Não dá para desfazer.` });
            if (!ok) return false;
            const r = await api.bibliotecaLimpar(manter, false);
            O.anunciar?.(`${r.apagados} versão(ões) apagada(s).`);
            if (aberto !== null) fechar(false);
            bus.emit('resultados:mudou');
            return true;
        } catch (e) { ui.toast(`Não consegui limpar. ${e.message || ''}`.trim(), { tipo: 'erro' }); return false; }
    }

    O.resultadosPainel = { abrir, fechar, aberto: () => aberto, limparAntigas };
    bus.on('capabilities', () => { if (aberto !== null) fechar(false); });
    raiz()?.addEventListener('keydown', e => { if (e.key === 'Escape' && aberto !== null) { e.stopPropagation(); fechar(); } });
})();
