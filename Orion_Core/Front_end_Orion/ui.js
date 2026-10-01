/* ============================================================
   ORION — ui.js | Navegação: sidebar, views, sessões, ajustes
   Carregado depois do script.js e usa o que ele expõe
   (Chat, abrirGrafo, toast, Prefs, Ceu, ttsMudo, CEREBRO).
   ============================================================ */

const OrionUI = (() => {

    const VIEWS = { config: 'view-config', integracoes: 'view-integracoes' };
    let _view = 'home';

    function showView(v) {
        if (v === 'memoria') {            // o grafo é um overlay próprio, por cima de tudo
            abrirGrafo();
            _syncNav('memoria');
            return;
        }
        for (const id of Object.values(VIEWS)) document.getElementById(id)?.classList.remove('open');

        const chat = v === 'chat';
        Chat.aberto = chat;
        document.getElementById('chat-view')?.classList.toggle('open', chat);
        if (chat) setTimeout(() => document.getElementById('orion-input')?.focus(), 200);
        if (VIEWS[v]) document.getElementById(VIEWS[v])?.classList.add('open');
        if (v === 'integracoes') _pollIntegracoes();

        _view = v;
        document.body.dataset.view = v;
        _syncNav(v);
    }

    function _syncNav(v = _view) {
        document.querySelectorAll('.sb-item').forEach(el => el.classList.toggle('active', el.dataset.view === v));
    }

    /* Esc: fecha a view atual e volta para o início. Retorna true se fechou algo. */
    function escapeView() {
        if (_view !== 'home') { showView('home'); return true; }
        document.activeElement?.blur?.();
        return false;
    }

    function currentView() { return _view; }

    /* ── Dropdown de modelo: espelha o <select id="sel-modelo"> oculto ── */
    function _configurarModelo() {
        const select = document.getElementById('sel-modelo');
        const wrap = document.getElementById('sb-select-modelo');
        const btn = document.getElementById('sb-select-btn');
        const label = document.getElementById('sb-select-label');
        const list = document.getElementById('sb-select-list');
        if (!select || !wrap || !btn || !label || !list) return;
        const opts = [...list.querySelectorAll('.sb-select-opt')];

        const sync = () => {
            const opt = opts.find(o => o.dataset.value === select.value) || opts[0];
            label.textContent = opt.firstChild.textContent.trim();
            btn.dataset.tip = 'Modelo: ' + label.textContent;
            opts.forEach(o => {
                o.classList.toggle('selected', o === opt);
                o.setAttribute('aria-selected', o === opt ? 'true' : 'false');
            });
        };
        sync();

        const fechar = () => wrap.classList.remove('open');
        btn.addEventListener('click', e => { e.stopPropagation(); wrap.classList.toggle('open'); });
        opts.forEach(o => o.addEventListener('click', () => {
            select.value = o.dataset.value;
            select.dispatchEvent(new Event('change'));
            sync();
            fechar();
        }));
        document.addEventListener('click', e => { if (!wrap.contains(e.target)) fechar(); });
        document.addEventListener('keydown', e => { if (e.key === 'Escape') fechar(); });
    }

    /* ── Sessões (/sessoes no cérebro) ── */
    async function novaConversa() {
        try {
            const d = await (await fetch(CEREBRO + '/sessoes', { method: 'POST' })).json();
            if (d.erro) { toast(d.erro, 3200); return; }
            Chat.limparDOM();
            showView('chat');
            _carregarSessoes();
        } catch (_) {
            toast('Cérebro fora do ar — não deu para criar a sessão.', 3600);
            showView('chat');
        }
    }

    async function abrirSessao(sid, somenteLeitura) {
        try {
            let msgs;
            if (somenteLeitura) {
                const r = await fetch(`${CEREBRO}/historico?sessao=${encodeURIComponent(sid)}`);
                msgs = (await r.json()).mensagens || [];
            } else {
                const r = await fetch(CEREBRO + '/sessoes/ativar', {
                    method: 'POST', headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ sessao_id: sid }),
                });
                const d = await r.json();
                if (d.erro) { toast(d.erro, 3200); return; }
                msgs = d.mensagens || [];
            }
            Chat.limparDOM();
            showView('chat');
            Chat.renderHistorico(msgs);
            _carregarSessoes();
            if (somenteLeitura) Chat.nota('Sessão antiga, somente leitura.');
        } catch (e) {
            toast('Falha ao abrir a conversa: ' + e.message, 3600);
        }
    }

    async function _carregarSessoes() {
        const lista = document.getElementById('sb-convs-list');
        if (!lista) return;
        try {
            const d = await (await fetch(CEREBRO + '/sessoes')).json();
            const sessoes = d.sessoes || [];
            lista.innerHTML = '';
            if (!sessoes.length) { lista.innerHTML = '<div class="sb-convs-hint">Nenhuma conversa ainda</div>'; return; }
            for (const s of sessoes) {
                const b = document.createElement('button');
                b.className = 'sb-conv' + (s.ativa ? ' active' : '');
                b.innerHTML = '<span class="sb-conv-title"></span><span class="sb-conv-when mono"></span>';
                b.querySelector('.sb-conv-title').textContent = s.titulo || 'Sem título';
                b.querySelector('.sb-conv-when').textContent = s.criada
                    ? new Date(s.criada).toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' }) : '';
                b.title = s.titulo || '';
                b.addEventListener('click', () => (s.ativa ? showView('chat') : abrirSessao(s.sessao_id, !!s.somente_leitura)));
                lista.appendChild(b);
            }
        } catch (_) {
            lista.innerHTML = '<div class="sb-convs-hint">Cérebro offline</div>';
        }
    }

    /* ── Configurações ── */
    function _configurarAjustes() {
        const tts = document.getElementById('cfg-tts');
        const syncTts = () => { if (tts) tts.checked = !ttsMudo(); };
        syncTts();
        tts?.addEventListener('change', () => { if (tts.checked === ttsMudo()) toggleTtsMute(); });
        document.addEventListener('orion:tts', syncTts);

        const mov = document.getElementById('cfg-motion');
        const aplicar = () => {
            const r = movimentoReduzido();
            document.body.classList.toggle('reduce-motion', r);
            if (mov) mov.checked = r;
            Ceu.setReduzido(r);
        };
        aplicar();
        mov?.addEventListener('change', () => { Prefs.set('movimento_reduzido', mov.checked ? '1' : '0'); aplicar(); });
    }

    /* ── Integrações (status real em /integracoes) ── */
    const INTEGRACOES = [
        { id: 'telegram', nome: 'Telegram',          desc: 'Fale com o Orion de qualquer lugar pelo bot.' },
        { id: 'voz_live', nome: 'Voz ao vivo',       desc: 'Conversa por voz em tempo real (Gemini Live).' },
        { id: 'mic',      nome: 'Microfone',         desc: 'Escuta local com palavra de ativação (mic_engine.py).' },
        { id: 'tts',      nome: 'Resposta por voz',  desc: 'Lê as respostas em voz alta.' },
        { id: 'enxame',   nome: 'Enxame de agentes', desc: 'Sub-agentes em paralelo para tarefas grandes.' },
        { id: 'upload',   nome: 'Anexos',            desc: 'Imagem, áudio e vídeo direto no chat.' },
    ];

    function _montarIntegracoes() {
        const grid = document.getElementById('integr-grid');
        if (!grid) return;
        grid.innerHTML = '';
        for (const it of INTEGRACOES) {
            const card = document.createElement('div');
            card.className = 'cfg-card integr-card';
            card.innerHTML =
                `<div class="integr-head"><span class="p-dot" id="integr-dot-${it.id}"></span>` +
                `<span class="integr-nome"></span></div>` +
                `<div class="integr-desc"></div>` +
                `<div class="integr-status mono" id="integr-st-${it.id}">—</div>`;
            card.querySelector('.integr-nome').textContent = it.nome;
            card.querySelector('.integr-desc').textContent = it.desc;
            grid.appendChild(card);
        }
    }

    async function _pollIntegracoes() {
        try {
            const r = await fetch(CEREBRO + '/integracoes');
            if (!r.ok) return;
            const d = await r.json();
            for (const it of INTEGRACOES) {
                const info = d[it.id];
                if (!info) continue;
                const dot = document.getElementById(`integr-dot-${it.id}`);
                const st = document.getElementById(`integr-st-${it.id}`);
                if (dot) {
                    dot.style.background = info.online ? COR.ok : 'rgba(163,173,194,0.25)';
                    dot.style.boxShadow = info.online ? `0 0 5px ${COR.ok}` : 'none';
                }
                if (st) st.textContent = info.status || (info.online ? 'ativa' : 'inativa');
            }
        } catch (_) { /* cérebro fora: cards ficam neutros */ }
    }

    function init() {
        document.body.dataset.view = 'home';
        if (Prefs.get('sb_recolhida') === '1') {
            document.body.classList.add('sb-collapsed');
            const b = document.getElementById('panel-toggle');
            if (b) b.dataset.tip = 'Expandir sidebar';
        }
        _configurarAjustes();
        _configurarModelo();
        _montarIntegracoes();
        _pollIntegracoes();
        setInterval(_pollIntegracoes, 12000);
        _carregarSessoes();
        setInterval(_carregarSessoes, 20000);   // pega o título automático da 1ª mensagem
    }

    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
    else init();

    return { showView, escapeView, currentView, novaConversa, syncNav: () => _syncNav() };
})();
window.OrionUI = OrionUI;
