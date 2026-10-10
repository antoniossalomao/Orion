/* ==========================================================================
   ORION — core.js | base compartilhada: DOM, barramento de eventos, preferências, estado
   ========================================================================== */
(function () {
    'use strict';
    const O = (window.Orion = window.Orion || {});

    /* ── DOM ───────────────────────────────────────────────────────────── */
    const $ = (sel, raiz = document) => raiz.querySelector(sel);
    const $$ = (sel, raiz = document) => Array.from(raiz.querySelectorAll(sel));

    /** <svg><use/></svg> do sprite do index.html */
    const icone = (nome, classe = '') =>
        `<svg class="i${classe ? ' ' + classe : ''}" aria-hidden="true"><use href="#i-${nome}"/></svg>`;

    /**
     * Cria elemento sem innerHTML com dado externo. `attrs`: class, text, html (SÓ confiável),
     * dataset:{}, on:{evento: fn}; o resto vira atributo (null/false remove).
     */
    function el(tag, attrs = {}, ...filhos) {
        const e = document.createElement(tag);
        for (const [k, v] of Object.entries(attrs || {})) {
            if (v == null || v === false) continue;
            if (k === 'class') e.className = v;
            else if (k === 'text') e.textContent = v;
            else if (k === 'html') e.innerHTML = v;
            else if (k === 'dataset') Object.assign(e.dataset, v);
            else if (k === 'on') for (const [ev, fn] of Object.entries(v)) e.addEventListener(ev, fn);
            else e.setAttribute(k, v === true ? '' : v);
        }
        for (const f of filhos.flat()) if (f != null) e.append(f.nodeType ? f : document.createTextNode(String(f)));
        return e;
    }

    /* ── barramento ────────────────────────────────────────────────────── */
    const ouvintes = new Map();
    const bus = {
        on(ev, fn) {
            if (!ouvintes.has(ev)) ouvintes.set(ev, new Set());
            ouvintes.get(ev).add(fn);
            return () => ouvintes.get(ev)?.delete(fn);
        },
        emit(ev, dados) {
            for (const fn of Array.from(ouvintes.get(ev) || [])) {
                try { fn(dados); } catch (e) { console.error(`[bus] ${ev}`, e); }
            }
        },
    };

    /* ── preferências (só conveniência de UI; o estado real vive no backend) ─ */
    let storage = null;
    try { storage = window.localStorage; storage.getItem('orion_probe'); } catch (_) { storage = null; }
    const prefs = O.Store.criar(
        { theme: 'noite', sb: 'expanded', density: 'confortavel', scale: 1, motion: 'system',
          model: 'auto', tts_mudo: false, sons: true, base_url: '', token: '', filtro_projeto: 'todos' },
        { persistir: ['theme', 'sb', 'density', 'scale', 'motion', 'model', 'tts_mudo', 'sons', 'base_url', 'token', 'filtro_projeto'], storage });

    const mqReduzido = window.matchMedia ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;
    const movimentoReduzido = () => {
        const m = prefs.get('motion');
        return m === 'reduced' || (m !== 'full' && !!mqReduzido?.matches);
    };

    // janela estreita (≤ 860 px): a barra lateral vira trilho de ícones, sem gaveta
    const mqEstreita = window.matchMedia ? window.matchMedia('(max-width: 860px)') : null;
    const estreita = () => !!mqEstreita?.matches;

    function aplicarPrefs() {
        const d = document.documentElement;
        d.dataset.theme = prefs.get('theme');
        d.dataset.density = prefs.get('density');
        d.dataset.sb = prefs.get('sb') === 'collapsed' || estreita() ? 'collapsed' : 'expanded';
        d.style.setProperty('--ui-scale', String(prefs.get('scale')));
        const m = prefs.get('motion');
        if (m === 'system') delete d.dataset.motion; else d.dataset.motion = m;
        bus.emit('prefs', prefs.todos());
    }
    prefs.assinar('*', aplicarPrefs);
    mqReduzido?.addEventListener?.('change', () => bus.emit('prefs', prefs.todos()));
    mqEstreita?.addEventListener?.('change', aplicarPrefs);

    /* ── estado do Orion (céu, pílula, aria) ───────────────────────────── */
    const ROTULO = { idle: 'Em espera', listening: 'Ouvindo', processing: 'Processando', speaking: 'Respondendo' };
    const estado = {
        atual: 'idle',
        rotulos: ROTULO,
        definir(s) {
            if (!ROTULO[s] || estado.atual === s) return;
            estado.atual = s;
            document.documentElement.dataset.estado = s;
            const l = $('#state-label');
            estado.pintar();
            bus.emit('estado', s);
        },
        /** aprovação pendente: a pílula do topo avisa (âmbar) mesmo com o Orion parado; sem nada acontecendo ela some */
        aprovacao(pendente) {
            if (!!pendente === estado.pendente) return;
            estado.pendente = !!pendente;
            estado.pintar();
        },
        pendente: false,
        pintar() {
            const d = document.documentElement, l = $('#state-label');
            if (estado.pendente) d.dataset.aprovacao = 'true'; else delete d.dataset.aprovacao;
            if (l) l.textContent = estado.pendente && estado.atual === 'idle' ? 'Aguardando aprovação' : ROTULO[estado.atual];
        },
    };

    /** fala para leitor de tela sem tirar o foco de ninguém */
    let ultimoAnuncio = '';
    function anunciar(texto) {
        const r = $('#sr-live');
        if (!r) return;
        // limpar antes força o leitor a repetir mensagens iguais em sequência
        r.textContent = '';
        ultimoAnuncio = String(texto);
        setTimeout(() => { if (r.textContent === '') r.textContent = ultimoAnuncio; }, 60);
    }

    const desktop = () => !!window.pywebview?.api;
    const interfaceDesktop = () => document.documentElement.classList.contains('shell-desktop');

    Object.assign(O, { $, $$, el, icone, bus, prefs, estado, anunciar, movimentoReduzido, aplicarPrefs,
                       desktop, interfaceDesktop, estreita });
})();
