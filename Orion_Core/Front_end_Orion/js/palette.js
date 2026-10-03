/* ==========================================================================
   ORION — palette.js | paleta de comandos (Ctrl+K): ir para, agir, trocar tema, achar conversa
   Padrão combobox + listbox (ARIA 1.2): o foco fica no campo; a opção ativa vai em
   aria-activedescendant. O app atrás fica `inert` enquanto a paleta está aberta.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, prefs } = O;
    const U = O.util;

    let raiz, campo, lista, anterior = null, aberta = false;
    let visiveis = [], sel = 0;

    const G = { ir: 'Ir para', acao: 'Ações', aparencia: 'Aparência', conversa: 'Conversas', perguntar: 'Perguntar' };
    const ORDEM_PADRAO = ['ir', 'acao', 'aparencia', 'conversa'];

    function comandos() {
        const A = O.acoes, ir = v => () => O.app.ir(v);
        const c = [
            { g: 'ir', rotulo: 'Início', icone: 'home', atalho: ['Alt', '1'], exec: ir('home') },
            { g: 'ir', rotulo: 'Chat', icone: 'chat', atalho: ['Alt', '2'], exec: ir('chat') },
            { g: 'ir', rotulo: 'Memória', icone: 'memory', atalho: ['Alt', '3'], exec: ir('memoria'), chaves: 'grafo' },
            { g: 'ir', rotulo: 'Integrações', icone: 'plug', atalho: ['Alt', '4'], exec: ir('integracoes') },
            { g: 'ir', rotulo: 'Configurações', icone: 'sliders', atalho: ['Alt', '5'], exec: ir('config'), chaves: 'preferências ajustes' },
            { g: 'acao', rotulo: 'Nova conversa', icone: 'plus', atalho: ['Ctrl', '⇧', 'O'], exec: () => O.sidebar.nova() },
            { g: 'acao', rotulo: 'Alternar barra lateral', icone: 'panel', atalho: ['Ctrl', 'B'], exec: () => O.sidebar.alternar(), chaves: 'recolher menu' },
            { g: 'acao', rotulo: O.voz?.ativa() ? 'Desligar voz ao vivo' : 'Ligar voz ao vivo', icone: 'wave', exec: () => O.voz.alternar(), chaves: 'microfone falar' },
            { g: 'acao', rotulo: prefs.get('tts_mudo') ? 'Ligar resposta por voz' : 'Desligar resposta por voz', icone: 'speaker', exec: () => A.alternarTts(), chaves: 'mudo tts som' },
            { g: 'acao', rotulo: 'Exportar conversa (Markdown)', icone: 'download', exec: () => A.exportar() },
            { g: 'acao', rotulo: 'Limpar histórico da sessão', icone: 'trash', exec: () => A.limpar() },
            { g: 'acao', rotulo: 'Atalhos de teclado', icone: 'keyboard', atalho: ['?'], exec: () => O.app.ir('config', { secao: 'cfg-atalhos' }), chaves: 'ajuda' },
            { g: 'aparencia', rotulo: 'Tema Noite', icone: 'palette', exec: () => A.tema('noite') },
            { g: 'aparencia', rotulo: 'Tema Grafite', icone: 'palette', exec: () => A.tema('grafite') },
            { g: 'aparencia', rotulo: 'Tema Alto contraste', icone: 'palette', exec: () => A.tema('contraste'), chaves: 'acessibilidade' },
            { g: 'aparencia', rotulo: prefs.get('density') === 'compacta' ? 'Densidade confortável' : 'Densidade compacta', icone: 'sliders', exec: () => A.densidade(prefs.get('density') === 'compacta' ? 'confortavel' : 'compacta') },
            { g: 'aparencia', rotulo: 'Aumentar texto', icone: 'plus', exec: () => A.escala(0.05) },
            { g: 'aparencia', rotulo: 'Diminuir texto', icone: 'minimize', exec: () => A.escala(-0.05) },
        ];
        return c;
    }

    function itensConversas() {
        return O.sidebar.sessoes().map(s => ({
            g: 'conversa', rotulo: s.titulo || 'Sem título', icone: 'chat', dica: s.somente_leitura ? 'somente leitura' : (s.criada ? U.quando(s.criada) : ''),
            exec: () => O.sidebar.abrir(s), peso: -2,
        }));
    }

    function calcular(consulta) {
        const q = consulta.trim();
        let todos = [...comandos(), ...itensConversas()];
        if (!q) {
            const conv = todos.filter(i => i.g === 'conversa').slice(0, 5);
            todos = [...todos.filter(i => i.g !== 'conversa'), ...conv];
            return ORDEM_PADRAO.flatMap(g => todos.filter(i => i.g === g));
        }
        const pontuados = todos.map(i => ({
            i, n: i.g === 'conversa' ? O.fuzzy.contem(q, i.rotulo).nota - 2 : O.fuzzy.nota(q, `${i.rotulo} ${i.chaves || ''}`),
        })).filter(x => x.n >= 0 || (x.i.g === 'conversa' && O.fuzzy.contem(q, x.i.rotulo).ok));
        const porGrupo = new Map();
        for (const x of pontuados) {
            if (!porGrupo.has(x.i.g)) porGrupo.set(x.i.g, []);
            porGrupo.get(x.i.g).push(x);
        }
        const grupos = [...porGrupo.entries()].map(([g, xs]) => ({ g, xs: xs.sort((a, b) => b.n - a.n).slice(0, 8), topo: Math.max(...xs.map(x => x.n)) }))
            .sort((a, b) => b.topo - a.topo);
        const saida = grupos.flatMap(gr => gr.xs.map(x => x.i));
        if (q.length >= 2) saida.push({ g: 'perguntar', rotulo: `Perguntar ao Orion: “${U.truncar(q, 60)}”`, icone: 'send-tg', exec: () => O.composer.sugerir(q), literal: true });
        return saida;
    }

    function rotuloComMarcas(item, consulta) {
        if (!consulta.trim() || item.literal) return [item.rotulo];
        const pos = new Set(item.g === 'conversa' ? O.fuzzy.contem(consulta, item.rotulo).marcas : O.fuzzy.marcas(consulta, item.rotulo));
        if (!pos.size) return [item.rotulo];
        const out = []; let acc = '', m = false;
        for (let i = 0; i < item.rotulo.length; i++) {
            const dentro = pos.has(i);
            if (dentro !== m && acc) { out.push(m ? el('mark', { text: acc }) : acc); acc = ''; }
            m = dentro; acc += item.rotulo[i];
        }
        if (acc) out.push(m ? el('mark', { text: acc }) : acc);
        return out;
    }

    function desenhar() {
        const consulta = campo.value;
        visiveis = calcular(consulta);
        sel = Math.min(sel, Math.max(0, visiveis.length - 1));
        if (!visiveis.length) {
            lista.replaceChildren(el('div', { class: 'palette-empty', role: 'presentation', text: 'Nada encontrado.' }));
            campo.setAttribute('aria-activedescendant', '');
            return;
        }
        const grupos = [];
        visiveis.forEach((it, idx) => {
            let g = grupos[grupos.length - 1];
            if (!g || g.g !== it.g) { g = { g: it.g, nos: [] }; grupos.push(g); }
            const atalho = it.atalho ? el('span', { class: 'pi-hint' }, ...it.atalho.map(k => el('kbd', { text: k }))) : null;
            const dica = it.dica ? el('span', { class: 'pi-hint', text: it.dica }) : null;
            const op = el('div', { class: 'palette-item', role: 'option', id: `pal-${idx}`, 'aria-selected': String(idx === sel), dataset: { idx } },
                el('span', { html: icone(it.icone || 'chevron-down') }),
                el('span', { class: 'pi-label' }, ...rotuloComMarcas(it, consulta)), dica, atalho);
            g.nos.push(op);
        });
        lista.replaceChildren(...grupos.map(g => {
            const id = `pg-${g.g}`;
            return el('div', { role: 'group', 'aria-labelledby': id }, el('div', { class: 'palette-group', id, text: G[g.g] }), ...g.nos);
        }));
        campo.setAttribute('aria-activedescendant', `pal-${sel}`);
    }

    function mover(novo, rolar = true) {
        if (!visiveis.length) return;
        sel = (novo + visiveis.length) % visiveis.length;
        for (const op of lista.querySelectorAll('[role="option"]')) op.setAttribute('aria-selected', String(+op.dataset.idx === sel));
        const ativo = $(`#pal-${sel}`);
        campo.setAttribute('aria-activedescendant', `pal-${sel}`);
        if (rolar) ativo?.scrollIntoView({ block: 'nearest' });
    }

    function executar(idx) {
        const it = visiveis[idx];
        if (!it) return;
        fechar(false);
        // depois de fechar: o comando decide para onde vai o foco
        setTimeout(() => it.exec(), 0);
    }

    function abrir() {
        if (aberta) return;
        aberta = true;
        anterior = document.activeElement;
        raiz.hidden = false;
        $('#app').inert = true;
        campo.value = '';
        sel = 0;
        desenhar();
        campo.focus();     // na hora: um Esc digitado no mesmo quadro tem de cair no campo
        requestAnimationFrame(() => { raiz.dataset.open = 'true'; });
    }
    function fechar(devolverFoco = true) {
        if (!aberta) return;
        aberta = false;
        raiz.dataset.open = 'false';
        $('#app').inert = false;
        setTimeout(() => { if (!aberta) raiz.hidden = true; }, 180);
        if (devolverFoco) anterior?.focus?.();
    }

    function init() {
        raiz = $('#palette'); campo = $('#palette-input'); lista = $('#palette-list');
        $('#palette-btn').addEventListener('click', abrir);
        campo.addEventListener('input', () => { sel = 0; desenhar(); });
        campo.addEventListener('keydown', e => {
            if (e.key === 'ArrowDown') { e.preventDefault(); mover(sel + 1); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); mover(sel - 1); }
            else if (e.key === 'Home') { e.preventDefault(); mover(0); }
            else if (e.key === 'End') { e.preventDefault(); mover(visiveis.length - 1); }
            else if (e.key === 'Enter') { e.preventDefault(); executar(sel); }
            else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); fechar(); }
            else if (e.key === 'Tab') e.preventDefault();
        });
        lista.addEventListener('mousemove', e => { const op = e.target.closest('[role="option"]'); if (op && +op.dataset.idx !== sel) mover(+op.dataset.idx, false); });
        lista.addEventListener('click', e => { const op = e.target.closest('[role="option"]'); if (op) executar(+op.dataset.idx); });
        raiz.addEventListener('mousedown', e => { if (e.target === raiz) fechar(); });
    }

    O.palette = { init, abrir, fechar, alternar: () => (aberta ? fechar() : abrir()), aberta: () => aberta };
})();
