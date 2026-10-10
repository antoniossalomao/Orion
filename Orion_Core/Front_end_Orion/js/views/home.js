/* ==========================================================================
   ORION — views/home.js | início: o céu é o palco, o conteúdo fica embaixo
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, bus, api, prefs } = O;
    const U = O.util;

    const SUGESTOES = [
        { icone: 'chat', titulo: 'Retomar uma ideia', desc: 'Continue de onde você parou.',
          texto: 'Me lembra o que eu estava fazendo nas últimas conversas?' },
        { icone: 'file', titulo: 'Organizar arquivos', desc: 'Planeje a organização de uma pasta.',
          texto: 'Me ajude a organizar minha pasta Downloads. Primeiro proponha um plano.' },
        { icone: 'bolt', titulo: 'Explorar um conceito', desc: 'Transforme uma dúvida em clareza.',
          texto: 'Explique a diferença entre diagrama de classes e de sequência, com um exemplo' },
        { icone: 'check', titulo: 'Planejar meu dia', desc: 'Veja sua agenda e defina prioridades.',
          texto: 'Resuma o que está na minha agenda hoje' },
    ];

    let timer = null, atualizando = false, ultimaAssinatura = '';

    function item(texto) { return el('span', { class: 'hs-item', text: texto }); }

    async function atualizarStatus() {
        if (O.app?.view() !== 'home' || atualizando) return;
        atualizando = true;
        try {
            let h = null;
            try { h = await api.health(); } catch (_) { /* detalhes indisponíveis: conexão fica na sidebar */ }
            const nomeModelo = (O.composer.MODELOS.find(m => m.id === prefs.get('model')) || O.composer.MODELOS[0]).nome;
            const textos = [api.estado().backend === 'orion' ? (api.estado().model === 'ready' ? 'Modelo do servidor' : 'Modelo indisponível') : nomeModelo];
            const vetores = h?.qdrant?.vetores ? Object.values(h.qdrant.vetores).reduce((s, x) => s + (+x || 0), 0) : 0;
            if (vetores) textos.push(`${U.fmtNum(vetores)} memórias`);
            const assinatura = JSON.stringify(textos);
            if (assinatura !== ultimaAssinatura) {
                $('#home-status').replaceChildren(...textos.map(item));
                ultimaAssinatura = assinatura;
            }
        } finally { atualizando = false; }
    }

    function cartao(s) {
        return el('button', { class: 'suggest', type: 'button', on: { click: () => O.composer.sugerir(s.texto) } },
            el('span', { class: 'suggest-icon', html: O.icone(s.icone), 'aria-hidden': 'true' }),
            el('strong', { text: s.titulo }), el('span', { class: 'suggest-desc', text: s.desc }));
    }

    function montarSugestoes() {
        $('#home-suggest').replaceChildren(...SUGESTOES.map(cartao));
        $('#suggest-grid').replaceChildren(...SUGESTOES.map(cartao));
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
