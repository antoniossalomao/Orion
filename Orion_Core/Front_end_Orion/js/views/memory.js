/* ==========================================================================
   ORION — views/memory.js | grafo 3D da memória (/grafo/completo em 3d-force-graph)
   · só é construído na primeira visita e PAUSA o render quando a tela não está ativa;
   · o canvas não é navegável por teclado, então busca + lista de resultados + painel de
     detalhes com vizinhos são o caminho equivalente (WCAG 2.1.1);
   · se o cérebro não responde, mostra dados de demonstração e AVISA que são demonstração.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, ui, api } = O;
    const U = O.util;

    const NUCLEO = 'orion:core';
    const RAIO = 70;
    const COR = { topico: 0xf5b95f, orion: 0x8fabff, user: 0x9aa6bd };
    const ROTULO_TIPO = { topico: 'Tópico', orion: 'Fala do Orion', user: 'Fala sua', nucleo: 'Núcleo' };
    const ehUsuario = ator => /^ant[oô]nio$/i.test(String(ator || '').trim());
    const tipoDe = n => (n.id === NUCLEO ? 'nucleo' : n.tipo === 'topico' ? 'topico' : ehUsuario(n.ator) ? 'user' : 'orion');

    let grafo = null, dados = null, demo = false, ativo = false, carregando = false, iniciando = false;
    let filtros = { topico: true, orion: true, user: true }, consulta = '', selecionado = null;
    const nos = new Map();      // id → nó (mesmo objeto entre reconstruções: mantém posição)
    let caixa, resultados, texNo = null;

    /* o 3d-force-graph (700 KB) só carrega na primeira visita à Memória. O bundle traz o próprio
       three.js (mesma revisão r128): soltar o marcador evita o aviso "múltiplas instâncias". */
    let promessaLib = null;
    function carregarBiblioteca() {
        if (typeof ForceGraph3D !== 'undefined') return Promise.resolve(true);
        if (!promessaLib) {
            promessaLib = new Promise(resolver => {
                const guardado = window.__THREE__;
                delete window.__THREE__;
                const s = document.createElement('script');
                s.src = 'vendor/3d-force-graph.min.js';
                s.onload = () => { if (guardado !== undefined) window.__THREE__ = guardado; resolver(true); };
                s.onerror = () => { if (guardado !== undefined) window.__THREE__ = guardado; promessaLib = null; resolver(false); };
                document.head.append(s);
            });
        }
        return promessaLib;
    }

    function webglOk() {
        try { const c = document.createElement('canvas'); return !!(c.getContext('webgl') || c.getContext('experimental-webgl')); } catch (_) { return false; }
    }

    function dadosDemo() {
        const temas = ['memória', 'sqlite', 'telegram', 'tailscale', 'voz', 'python', 'fase 0', 'mcp', 'backup', 'notebook', 'obsidian', 'agenda'];
        const frases = ['Exportar as tabelas pessoais antes da venda do PC', 'Memória nova em SQLite com busca híbrida', 'Bot do Telegram como canal principal no celular',
            'Acesso de fora pela rede do Tailscale', 'Voz masculina com palavra de ativação', 'Backup diário do arquivo de memória'];
        const nodes = temas.map(t => ({ id: `topico:${t}`, tipo: 'topico', label: t }));
        const links = [];
        for (let i = 0; i < 48; i++) {
            const id = `evento:demo${i}`;
            nodes.push({ id, tipo: 'evento', label: frases[i % frases.length], ator: i % 2 ? 'Antônio' : 'Orion' });
            for (let k = 0; k < 2; k++) links.push({ source: id, target: `topico:${temas[(i * 3 + k * 5) % temas.length]}` });
        }
        return { nodes, links };
    }

    /* ── texturas e objetos 3D ─────────────────────────────────────────── */
    function textura() {
        if (texNo) return texNo;
        const sz = 64, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const gr = ctx.createRadialGradient(sz / 2, sz / 2, 0, sz / 2, sz / 2, sz / 2);
        gr.addColorStop(0, 'rgba(255,255,255,1)'); gr.addColorStop(0.2, 'rgba(255,255,255,0.8)');
        gr.addColorStop(0.5, 'rgba(255,255,255,0.16)'); gr.addColorStop(1, 'rgba(255,255,255,0)');
        ctx.fillStyle = gr; ctx.fillRect(0, 0, sz, sz);
        texNo = new THREE.CanvasTexture(cv);
        return texNo;
    }
    function objetoNo(n) {
        const g = new THREE.Group();
        const spr = (tam, cor, op) => {
            const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: textura(), color: cor, blending: THREE.AdditiveBlending, transparent: true, opacity: op, depthWrite: false }));
            s.scale.set(tam, tam, 1);
            g.add(s);
        };
        if (n.id === NUCLEO) {
            spr(30, new THREE.Color(0x24366b), 0.12); spr(14, new THREE.Color(0x8fabff), 0.32);
            spr(6, new THREE.Color(0xdfe7ff), 0.75); spr(2.6, new THREE.Color(0xffffff), 1);
            return g;
        }
        const tipo = tipoDe(n), topico = tipo === 'topico', cor = new THREE.Color(COR[tipo]);
        const base = (topico ? 2.4 : 1.4) * (1 + Math.log2(Math.max(n._grau, 1)) * (topico ? 0.45 : 0.2));
        spr(base * 3, cor, topico ? 0.05 : 0.03); spr(base * 1.7, cor, topico ? 0.16 : 0.1); spr(base, cor, topico ? 0.92 : 0.8);
        if (selecionado && selecionado.id === n.id) { spr(base * 3.2, new THREE.Color(0xffffff), 0.14); spr(base * 1.05, new THREE.Color(0xffffff), 1); }
        return g;
    }

    const forcaEsfera = (() => {
        let ns = [];
        function f(alpha) {
            for (const n of ns) {
                if (n.id === NUCLEO) continue;
                const d = Math.hypot(n.x || 0, n.y || 0, n.z || 0) || 0.001;
                const k = (RAIO / d - 1) * 0.9 * alpha;
                n.vx = (n.vx || 0) + (n.x || 0) * k; n.vy = (n.vy || 0) + (n.y || 0) * k; n.vz = (n.vz || 0) + (n.z || 0) * k;
            }
        }
        f.initialize = x => { ns = x; };
        return f;
    })();

    /* ── dados → grafo ─────────────────────────────────────────────────── */
    function preparar() {
        const grau = {};
        const idsValidos = new Set(dados.nodes.map(n => n.id));
        // link para nó inexistente derruba o 3d-force-graph: filtra aqui
        const ligacoes = (dados.links || []).map(l => ({ source: l.source?.id ?? l.source, target: l.target?.id ?? l.target, rel: l.rel }))
            .filter(l => idsValidos.has(l.source) && idsValidos.has(l.target));
        for (const l of ligacoes) { grau[l.source] = (grau[l.source] || 0) + 1; grau[l.target] = (grau[l.target] || 0) + 1; }
        const lista = dados.nodes.map(n => {
            const existente = nos.get(n.id);
            const no = existente ? Object.assign(existente, n) : { ...n };
            no._grau = grau[n.id] || 1;
            nos.set(n.id, no);
            return no;
        });
        const nucleo = nos.get(NUCLEO) || { id: NUCLEO, tipo: 'nucleo', label: 'Orion', fx: 0, fy: 0, fz: 0 };
        nos.set(NUCLEO, nucleo);
        const casca = lista.filter(n => n.x == null);
        casca.forEach((n, i) => {
            const phi = Math.acos(1 - 2 * (i + 0.5) / casca.length), th = Math.PI * (1 + Math.sqrt(5)) * i;
            n.x = RAIO * Math.sin(phi) * Math.cos(th); n.y = RAIO * Math.sin(phi) * Math.sin(th); n.z = RAIO * Math.cos(phi);
        });
        return { lista, ligacoes };
    }

    let base = null;
    function visivel() {
        const { lista, ligacoes } = base;
        const q = consulta.trim();
        const ok = n => filtros[tipoDe(n)] && (!q || O.fuzzy.contem(q, n.label || n.id).ok);
        const ns = lista.filter(ok);
        const ids = new Set(ns.map(n => n.id));
        const ls = ligacoes.filter(l => ids.has(l.source) && ids.has(l.target)).map(l => ({ ...l }));
        const topicos = ns.filter(n => n.tipo === 'topico');
        ns.push(nos.get(NUCLEO));
        // curvatura determinística por tópico: o desenho não muda a cada filtro
        for (const t of topicos) ls.push({ source: NUCLEO, target: t.id, rel: 'nucleo', _curv: 0.15 + (t.id.length % 7) * 0.03, _rot: t.id.length % 6 });
        return { nodes: ns, links: ls };
    }

    function aplicar() {
        if (!grafo || !base) return;
        const g = visivel();
        grafo.graphData(g);
        $('#mem-stats').innerHTML = '';
        $('#mem-stats').append(el('span', {}, el('b', { text: U.fmtNum(g.nodes.length - 1) }), g.nodes.length - 1 === 1 ? ' nó' : ' nós'), el('span', {}, el('b', { text: U.fmtNum(g.links.length) }), g.links.length === 1 ? ' ligação' : ' ligações'));
        desenharResultados();
    }

    /* ── construção do 3D ──────────────────────────────────────────────── */
    function construir() {
        const box = $('#grafo-container');
        if (typeof ForceGraph3D === 'undefined' || !webglOk()) {
            $('#mem-overlay').replaceChildren(el('div', { class: 'empty' }, el('span', { html: icone('memory') }),
                el('h3', { text: 'Gráfico 3D indisponível' }),
                el('p', { text: 'Este navegador não conseguiu iniciar o WebGL. A busca e os detalhes abaixo continuam funcionando.' })));
            return false;
        }
        const w = box.clientWidth || 900, h = box.clientHeight || 580;
        grafo = ForceGraph3D({ controlType: 'orbit' })(box)
            .width(w).height(h).backgroundColor('rgba(0,0,0,0)').showNavInfo(false)
            .nodeLabel(n => U.escapar(n.id === NUCLEO ? 'Orion' : (n.label || n.id)))
            .nodeThreeObject(objetoNo).nodeThreeObjectExtend(false)
            .linkColor(l => (l.rel === 'nucleo' ? 'rgba(143,171,255,0.28)' : 'rgba(0,0,0,0)'))
            .linkWidth(l => (l.rel === 'nucleo' ? 0.4 : 0))
            .linkCurvature(l => l._curv || 0).linkCurveRotation(l => l._rot || 0)
            .d3AlphaDecay(0.04).d3VelocityDecay(0.35).cooldownTicks(200)
            .onEngineStop(() => {
                const c = grafo?.controls();
                if (c && !O.movimentoReduzido()) { c.autoRotate = true; c.autoRotateSpeed = 0.28; }
            })
            .onNodeClick(n => selecionar(n, { voar: false, foco: false }))
            .onBackgroundClick(() => fecharDetalhe());
        // estrelas ao fundo
        const pos = new Float32Array(1200 * 3);
        for (let i = 0; i < 1200; i++) {
            const ph = Math.acos(2 * Math.random() - 1), th = Math.random() * Math.PI * 2, r = 380 + Math.random() * 200;
            pos[i * 3] = r * Math.sin(ph) * Math.cos(th); pos[i * 3 + 1] = r * Math.sin(ph) * Math.sin(th); pos[i * 3 + 2] = r * Math.cos(ph);
        }
        const geo = new THREE.BufferGeometry();
        geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
        grafo.scene().add(new THREE.Points(geo, new THREE.PointsMaterial({ color: 0xc8d4ff, size: 0.55, transparent: true, opacity: 0.28, depthWrite: false })));
        grafo.d3Force('radial-esfera', forcaEsfera);
        grafo.d3Force('charge').strength(-10);
        grafo.d3Force('link')?.distance(RAIO * 0.8).strength(0.03);
        grafo.cameraPosition({ x: 0, y: 0, z: RAIO * 3.6 });
        const c = grafo.controls();
        if (c) { c.enableDamping = true; c.dampingFactor = 0.07; c.addEventListener?.('start', () => { c.autoRotate = false; }); }
        new ResizeObserver(() => { if (grafo && box.clientWidth) grafo.width(box.clientWidth).height(box.clientHeight); }).observe(box);
        return true;
    }

    async function carregar() {
        if (carregando) return;
        carregando = true;
        const sobre = $('#mem-overlay');
        sobre.replaceChildren(el('div', { class: 'empty', role: 'status' }, el('span', { class: 'status-dot', dataset: { state: 'loading' } }), el('p', { text: 'Carregando memória…' })));
        $('#mem-banner').hidden = true;
        try {
            const d = await api.grafo(500);
            if (!d.nodes?.length) throw new Error('vazio');
            dados = d; demo = false;
        } catch (e) {
            dados = dadosDemo(); demo = true;
            const motivo = e.message === 'vazio' ? 'A memória ainda não tem nós.' : (e.message || 'O cérebro não respondeu.');
            const faixa = $('#mem-banner');
            faixa.replaceChildren(el('div', { class: 'banner banner-warn', role: 'status' }, el('span', { html: icone('alert') }),
                el('div', {}, el('strong', { text: 'Dados de demonstração. ' }), motivo),
                el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Tentar de novo', on: { click: carregar } })));
            faixa.hidden = false;
        }
        base = preparar();
        sobre.replaceChildren();
        aplicar();
        carregando = false;
    }

    /* ── seleção, detalhe e resultados ─────────────────────────────────── */
    function vizinhos(n) {
        const ids = new Set();
        for (const l of base.ligacoes) {          // do grafo inteiro, não só do que a busca deixou visível
            const s = l.source, t = l.target;
            if (s === n.id) ids.add(t); else if (t === n.id) ids.add(s);
        }
        return [...ids].map(id => nos.get(id)).filter(x => x && x.id !== NUCLEO);
    }

    function voarPara(n) {
        if (!grafo || n.x == null) return;
        const d = Math.hypot(n.x, n.y, n.z) || 1, k = 1 + 55 / d;
        grafo.cameraPosition({ x: n.x * k, y: n.y * k, z: n.z * k }, n, O.movimentoReduzido() ? 0 : 900);
    }

    function selecionar(n, { voar = true, foco = true } = {}) {
        selecionado = n;
        grafo?.nodeThreeObject(objetoNo);      // redesenha o anel de seleção
        if (voar) voarPara(n);
        const tipo = tipoDe(n);
        $('#mem-detail-tipo').textContent = ROTULO_TIPO[tipo];
        const corpo = $('#mem-detail-body');
        const filhos = [];
        if (tipo === 'nucleo') filhos.push(el('h3', { tabindex: '-1', text: 'Orion' }), el('p', { class: 'meta', text: 'Todos os tópicos de memória partem daqui.' }));
        else {
            filhos.push(el('h3', { tabindex: '-1', text: n.label || n.id }));
            const meta = el('div', { class: 'meta' });
            if (n.ts) meta.append(el('span', { text: new Date(n.ts).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }) }));
            meta.append(el('span', { text: `${n._grau} ${n._grau === 1 ? 'ligação' : 'ligações'}` }));
            filhos.push(meta);
            const viz = vizinhos(n).slice(0, 14);
            if (viz.length) {
                filhos.push(el('div', { class: 'card-sub', text: 'Conectado a', style: 'margin-bottom:.3rem' }),
                    el('div', { class: 'mem-links' }, ...viz.map(v => el('button', { type: 'button', on: { click: () => selecionar(v, { foco: true }) } },
                        el('span', { class: `leg-dot leg-${tipoDe(v)}`, 'aria-hidden': 'true' }), el('span', { class: 'conv-title', text: v.label || v.id })))));
            }
        }
        corpo.replaceChildren(...filhos);
        $('#mem-detail').dataset.open = 'true';
        if (foco) corpo.querySelector('h3')?.focus({ preventScroll: true });
        O.anunciar(`${ROTULO_TIPO[tipo]}: ${U.truncar(n.label || n.id, 120)}`);
    }

    function fecharDetalhe() {
        const aberto = $('#mem-detail').dataset.open === 'true';
        if (!aberto) return false;
        $('#mem-detail').dataset.open = 'false';
        selecionado = null;
        grafo?.nodeThreeObject(objetoNo);
        return true;
    }

    function achados() {
        const q = consulta.trim();
        if (!q || !base) return [];
        return O.fuzzy.buscar(base.lista.filter(n => filtros[tipoDe(n)]), q, n => n.label || n.id).slice(0, 8);
    }
    function desenharResultados() {
        const lista = achados();
        resultados.hidden = !lista.length;
        resultados.replaceChildren(...lista.map(n => el('button', { type: 'button', class: 'mem-result', on: { click: () => selecionar(n) } },
            el('span', { class: `leg-dot leg-${tipoDe(n)}`, 'aria-hidden': 'true' }), el('span', { class: 'conv-title', text: n.label || n.id }))));
        resultados.setAttribute('aria-label', `${lista.length} resultados`);
    }

    /* ── API da view ───────────────────────────────────────────────────── */
    O.views = O.views || {};
    O.views.memoria = {
        init() {
            caixa = $('.mem-body');
            resultados = el('div', { class: 'mem-results', role: 'group', hidden: true });
            caixa.append(resultados);
            const busca = $('#mem-search');
            busca.addEventListener('input', U.debounce(() => { consulta = busca.value; aplicar(); }, 150));
            busca.addEventListener('keydown', e => {
                if (e.key === 'Enter') { const [primeiro] = achados(); if (primeiro) { e.preventDefault(); selecionar(primeiro); } }
                if (e.key === 'Escape' && busca.value) { e.stopPropagation(); busca.value = ''; consulta = ''; aplicar(); }
            });
            $('#mem-filters').addEventListener('click', e => {
                const b = e.target.closest('[data-filtro]');
                if (!b) return;
                const on = b.getAttribute('aria-pressed') !== 'true';
                b.setAttribute('aria-pressed', String(on));
                filtros[b.dataset.filtro] = on;
                aplicar();
            });
            $('#mem-reload').addEventListener('click', carregar);
            $('#mem-detail-close').addEventListener('click', () => { fecharDetalhe(); busca.focus(); });
        },
        async ativar() {
            ativo = true;
            if (!grafo && !dados) {
                if (iniciando) return;
                iniciando = true;
                await carregarBiblioteca();
                construir();
                iniciando = false;
                carregar();
            } else grafo?.resumeAnimation?.();
            requestAnimationFrame(() => { const box = $('#grafo-container'); grafo?.width(box.clientWidth).height(box.clientHeight); });
        },
        desativar() { ativo = false; grafo?.pauseAnimation?.(); },
        escape() { return fecharDetalhe(); },
        ativoAgora: () => ativo,
    };
})();
