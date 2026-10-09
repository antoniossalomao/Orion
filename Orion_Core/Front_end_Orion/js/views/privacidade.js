/* ==========================================================================
   ORION — views/privacidade.js | o que saiu do computador (regra 47)
   Tudo vem de `GET /privacidade` (orion/app.py): por dia e provedor, quantos envios, quantos bytes e
   de que tipo (texto, imagem, áudio). Nunca o conteúdo. Barras empilhadas em CSS com a geometria de
   `charts.empilhadas` (testada em Node); só `textContent`.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, api } = O;
    const C = O.charts;
    const L = O.painelLogica;

    const CORES = 6;   // .priv-c0…c5 no CSS; o provedor guarda a cor pela posição na lista
    let dados = null, falhou = false, dias = 7, pedido = 0;

    const diaCurto = d => `${d.slice(8, 10)}/${d.slice(5, 7)}`;
    const vazio = texto => el('p', { class: 'painel-vazio', text: texto });
    const cartao = (id, titulo, dica, ...filhos) => el('section', { class: 'card painel-card', 'aria-labelledby': `pv-${id}`, dataset: { id } },
        el('div', { class: 'card-head' }, el('h3', { class: 'card-title', id: `pv-${id}`, text: titulo }),
            dica ? el('span', { class: 'card-sub', text: dica }) : null),
        ...filhos);

    function totais(d) {
        const por = {};
        for (const dia of d.serie) {
            for (const [nome, p] of Object.entries(dia.provedores || {})) {
                const t = (por[nome] = por[nome] || { envios: 0, bytes: 0, tipos: {} });
                t.envios += p.envios; t.bytes += p.bytes;
                for (const [tipo, n] of Object.entries(p.tipos || {})) t.tipos[tipo] = (t.tipos[tipo] || 0) + n;
            }
        }
        return por;
    }

    function graficoEl(d) {
        if (!d.provedores.length) return vazio(`Nada saiu do computador nos últimos ${d.dias} dias.`);
        const barras = C.empilhadas(d.serie, d.provedores);
        return el('div', {},
            el('ul', { class: 'priv-grafico', 'aria-label': `Envios por dia e provedor, últimos ${d.dias} dias` }, ...barras.map(b => {
                const partes = b.segmentos.map(s => `${s.nome} ${s.valor}`).join(', ');
                return el('li', { class: 'priv-dia', dataset: { dia: b.dia, total: String(b.total) } },
                    el('span', { class: 'priv-coluna', style: `height:${b.altura}%`, 'aria-hidden': 'true' },
                        ...b.segmentos.map(s => el('span', { class: `priv-seg priv-c${s.cor % CORES}`, style: `height:${s.pct}%`, dataset: { provedor: s.nome } }))),
                    el('span', { class: 'priv-n mono', 'aria-hidden': 'true', text: String(b.total) }),
                    el('span', { class: 'priv-r', 'aria-hidden': 'true', text: diaCurto(b.dia) }),
                    el('span', { class: 'sr-only', text: `${diaCurto(b.dia)}: ${b.total} envio(s)${partes ? ` (${partes})` : ''}` }));
            })),
            el('ul', { class: 'priv-legenda', 'aria-label': 'Provedores' }, ...d.provedores.map((nome, i) =>
                el('li', {}, el('span', { class: `priv-cor priv-c${i % CORES}`, 'aria-hidden': 'true' }), el('span', { class: 'mono', text: nome })))));
    }

    function totaisEl(d) {
        const por = totais(d);
        const nomes = Object.keys(por).sort((a, b) => por[b].envios - por[a].envios);
        if (!nomes.length) return vazio('Sem envios no período.');
        return el('ul', { class: 'painel-lista', 'aria-label': 'Totais por provedor' }, ...nomes.map(nome => {
            const t = por[nome];
            const tipos = Object.entries(t.tipos).map(([k, n]) => `${k} ×${n}`).join(' · ');
            return el('li', { class: 'painel-item', dataset: { provedor: nome } },
                el('div', { class: 'painel-item-top' }, el('strong', { class: 'mono', text: nome }),
                    el('span', { class: 'badge badge-muted', text: `${t.envios} envio(s) · ${L.bytes(t.bytes)}` })),
                el('small', { text: tipos }));
        }));
    }

    function hojeEl(d) {
        if (!d.hoje.length) return vazio('Nada saiu hoje.');
        const th = texto => el('th', { scope: 'col', text: texto });
        return el('div', { class: 'tabela-rola', tabindex: '0', role: 'region', 'aria-label': 'Tabela do que saiu hoje' }, el('table', { class: 'tabela' },
            el('caption', { class: 'sr-only', text: 'O que saiu do computador hoje, mais recente primeiro' }),
            el('thead', {}, el('tr', {}, th('Hora'), th('Provedor'), th('Tipo'), th('Conteúdo'), th('Tamanho'), th('Resultado'))),
            el('tbody', {}, ...d.hoje.map(x => el('tr', { dataset: { provedor: x.provedor } },
                el('td', { class: 'mono', text: x.hora }), el('th', { scope: 'row', class: 'mono', text: x.provedor }),
                el('td', { text: x.tipo }), el('td', { text: x.conteudo }), el('td', { class: 'mono', text: L.bytes(x.bytes) }),
                el('td', { text: x.ok ? 'ok' : 'falhou' }))))));
    }

    function desenhar() {
        const raiz = $('#privacidade-corpo');
        raiz.setAttribute('aria-busy', 'false');
        if (!dados) {
            raiz.replaceChildren(el('div', { class: 'banner banner-warn', role: 'status' }, el('span', { html: icone('alert') }),
                el('div', {}, el('strong', { text: falhou ? 'O cérebro não respondeu. ' : 'Consultando… ' }),
                    falhou ? 'Confira a conexão em Configurações › Conexão e atualize.' : '')));
            return;
        }
        const envios = dados.serie.reduce((s, d) => s + d.envios, 0), bytes = dados.serie.reduce((s, d) => s + d.bytes, 0);
        raiz.replaceChildren(
            el('div', { class: 'banner banner-info', role: 'status' }, el('span', { html: icone('shield') }),
                el('div', {}, el('strong', { text: `${envios} envio(s), ${L.bytes(bytes)} em ${dados.dias} dias. ` },
                ), 'Só tamanho e tipo: o conteúdo nunca é guardado. O modelo local não conta (não sai daqui).')),
            el('div', { class: 'grid grid-2 painel-grade' },
                cartao('grafico', 'Por dia', `últimos ${dados.dias} dias`, graficoEl(dados)),
                cartao('totais', 'Por provedor', 'no período', totaisEl(dados)),
                cartao('hoje', 'O que saiu hoje', `${dados.hoje.length} envio(s)`, hojeEl(dados))));
    }

    async function atualizar() {
        const meu = ++pedido;
        try {
            const d = await api.privacidade(dias);
            if (meu !== pedido) return;
            dados = d; falhou = false;
        } catch (_) { if (meu === pedido) falhou = true; }
        desenhar();
    }

    O.views = O.views || {};
    O.views.privacidade = {
        init() {
            $('#privacidade-refresh').addEventListener('click', async e => {
                const b = e.currentTarget; b.disabled = true;
                await atualizar();
                b.disabled = false;
            });
            $('#privacidade-dias').addEventListener('change', e => { dias = Number(e.currentTarget.value) || 7; atualizar(); });
        },
        ativar() { atualizar(); },
    };
})();
