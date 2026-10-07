/* ==========================================================================
   ORION — views/integrations.js | canais e serviços ligados ao Orion (estado real de /integracoes)
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, api, prefs } = O;

    const INTEGRACOES = [
        { id: 'telegram', nome: 'Telegram', icone: 'send-tg', desc: 'Fale com o Orion de qualquer lugar pelo bot. É o canal principal no celular.' },
        { id: 'voz_live', nome: 'Voz ao vivo', icone: 'wave', desc: 'Conversa por voz em tempo real (Gemini Live), direto do navegador.',
          acao: () => ({ rotulo: O.voz.ativa() ? 'Desligar' : 'Ligar', fn: () => { O.voz.alternar(); setTimeout(desenharAcoes, 400); } }) },
        { id: 'mic', nome: 'Microfone', icone: 'mic', desc: 'Escuta local com palavra de ativação.',
          dicaInativa: 'Ligar é feito no computador onde o cérebro roda.' },
        { id: 'tts', nome: 'Resposta por voz', icone: 'speaker', desc: 'Lê as respostas em voz alta no computador onde o cérebro roda.',
          acao: () => ({ rotulo: prefs.get('tts_mudo') ? 'Ligar' : 'Desligar', fn: () => O.acoes.alternarTts() }) },
        { id: 'enxame', nome: 'Enxame de agentes', icone: 'swarm', desc: 'Sub-agentes em paralelo para tarefas grandes.' },
        { id: 'upload', nome: 'Anexos', icone: 'image', desc: 'Imagem, áudio e vídeo direto no chat: clipe, colar ou arrastar.' },
    ];

    let timer = null, estado = {}, falhou = false;

    /** o cérebro devolve texto de máquina ("mic_engine.py parado"): troca nome de arquivo por palavra e põe maiúscula */
    function humanizar(t) {
        const s = String(t ?? '').replace(/\bmic_engine\.py\b/gi, 'microfone').replace(/\b[\w-]+\.py\b/g, 'serviço').trim();
        return s ? s[0].toUpperCase() + s.slice(1) : s;
    }

    function cartao(it) {
        const info = estado[it.id];
        const online = !!info?.online;
        const selo = !info ? el('span', { class: 'badge badge-muted', text: falhou ? 'Sem resposta' : '…' })
            : el('span', { class: `badge ${online ? 'badge-ok' : 'badge-muted'}`, text: online ? 'Ativa' : 'Inativa' });
        const acao = it.acao ? it.acao() : null;
        return el('article', { class: 'card integ-card', 'aria-labelledby': `ig-${it.id}`, dataset: { id: it.id } },
            el('div', { class: 'integ-head' }, el('span', { class: 'integ-ico', html: icone(it.icone) }),
                el('h3', { class: 'integ-name', id: `ig-${it.id}`, text: it.nome }), selo),
            el('p', { class: 'integ-desc', text: it.desc }),
            el('div', { class: 'integ-status' }, el('span', { class: 'integ-status-txt', text: info ? humanizar(info.status || (online ? 'ativa' : 'inativa')) + (!online && it.dicaInativa ? `. ${it.dicaInativa}` : '') : (falhou ? 'Cérebro não respondeu' : 'Consultando…') }),
                acao && info ? el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: acao.rotulo, on: { click: acao.fn } }) : null));
    }

    function desenharAcoes() { desenhar(); }
    function desenhar() {
        const grade = $('#integ-grid');
        grade.setAttribute('aria-busy', 'false');
        const topo = falhou ? [el('div', { class: 'banner banner-warn', role: 'status', style: 'grid-column:1/-1' }, el('span', { html: icone('alert') }),
            el('div', {}, el('strong', { text: 'O cérebro não respondeu. ' }), 'Os estados abaixo podem estar desatualizados.'))] : [];
        grade.replaceChildren(...topo, ...INTEGRACOES.map(cartao));
    }

    async function atualizar() {
        try { estado = await api.integracoes(); falhou = false; }
        catch (_) { falhou = true; }
        desenhar();
    }

    O.views = O.views || {};
    O.views.integracoes = {
        init() {
            $('#integ-refresh').addEventListener('click', async e => {
                const b = e.currentTarget; b.disabled = true;
                await atualizar();
                b.disabled = false;
            });
            O.bus.on('prefs', () => { if (timer) desenhar(); });
            desenhar();
        },
        ativar() {
            atualizar();
            clearInterval(timer);
            timer = setInterval(() => { if (!document.hidden) atualizar(); }, 12000);
        },
        desativar() { clearInterval(timer); timer = null; },
    };
})();
