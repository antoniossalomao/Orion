/* ==========================================================================
   ORION — views/home.js | início: o céu é o palco, o conteúdo fica embaixo
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, bus, api, prefs } = O;
    const U = O.util;

    const SUGESTOES = [
        { rotulo: 'Retomar de onde parei', titulo: 'O que eu estava fazendo?', desc: 'Retoma o contexto das últimas conversas.',
          texto: 'Me lembra o que eu estava fazendo nas últimas conversas?' },
        { rotulo: 'Limpar Downloads antigos', titulo: 'Organizar uma pasta', desc: 'Peça uma ação no computador: você aprova antes de rodar.',
          texto: 'Apague os arquivos antigos da minha pasta Downloads' },
        { rotulo: 'Explicar um conceito', titulo: 'Explicar um conceito', desc: 'Com exemplo, tabela e código quando ajudar.',
          texto: 'Explique a diferença entre diagrama de classes e de sequência, com um exemplo' },
        { rotulo: 'Resumir meu dia', titulo: 'Resumo do dia', desc: 'Agenda e pendências em poucas linhas.',
          texto: 'Resuma o que está na minha agenda hoje' },
    ];

    let timer = null;

    function chip(texto, estado) {
        return el('span', { class: 'chip' },
            estado ? el('span', { class: 'status-dot', dataset: { state: estado }, 'aria-hidden': 'true' }) : null, texto);
    }

    async function atualizarStatus() {
        const caixa = $('#home-status');
        const nomeModelo = (O.composer.MODELOS.find(m => m.id === prefs.get('model')) || O.composer.MODELOS[0]).nome;
        const chips = [];
        let h = null;
        try { h = await api.health(); } catch (_) { /* legado sem /health completo */ }
        const r = h ? { ok: true } : await api.ping();
        chips.push(r.ok ? chip('Cérebro conectado', 'ok') : chip('Cérebro offline', 'danger'));
        chips.push(chip(`Modelo: ${nomeModelo}`));
        const vetores = h?.qdrant?.vetores ? Object.values(h.qdrant.vetores).reduce((s, x) => s + (+x || 0), 0) : 0;
        if (vetores) chips.push(chip(`${U.fmtNum(vetores)} memórias`));
        if (h?.latencia_ultimo_chat_ms) chips.push(chip(`Último chat: ${U.fmtMs(h.latencia_ultimo_chat_ms)}`));
        caixa.replaceChildren(...chips);
    }

    function montarSugestoes() {
        const caixa = $('#home-suggest');
        caixa.replaceChildren(...SUGESTOES.map(s => el('button', { class: 'chip', type: 'button', text: s.rotulo,
            on: { click: () => O.composer.sugerir(s.texto) } })));
        const grade = $('#suggest-grid');
        grade.replaceChildren(...SUGESTOES.map(s => el('button', { class: 'suggest', type: 'button', on: { click: () => O.composer.sugerir(s.texto) } },
            el('strong', { text: s.titulo }), el('span', { text: s.desc }))));
    }

    function rotuloEstrela() {
        const caixa = $('#star-label');
        bus.on('sky:hover', h => {
            if (!h) { caixa.dataset.show = 'false'; return; }
            caixa.querySelector('strong').textContent = h.nome;
            caixa.querySelector('span').textContent = h.info;
            caixa.style.left = `${h.x}px`;
            caixa.style.top = `${h.y}px`;
            caixa.dataset.show = 'true';
        });
    }

    O.views = O.views || {};
    O.views.home = {
        init() {
            montarSugestoes();
            rotuloEstrela();
            $('#home-saudacao').textContent = U.saudacao();
            bus.on('conn', atualizarStatus);
            prefs.assinar('model', atualizarStatus);
        },
        ativar() {
            $('#home-saudacao').textContent = U.saudacao();
            atualizarStatus();
            clearInterval(timer);
            timer = setInterval(() => { if (!document.hidden) atualizarStatus(); }, 30000);
        },
        desativar() { clearInterval(timer); $('#star-label').dataset.show = 'false'; },
    };
})();
