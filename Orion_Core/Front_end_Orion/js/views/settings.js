/* ==========================================================================
   ORION — views/settings.js | aparência, voz, modelo, conexão, atividade, atalhos, sobre
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, bus, ui, api, prefs } = O;
    const U = O.util, CH = O.charts;

    const MAX_LAT = 24;
    let latencias = [], timers = [], ativo = false;

    /* ── aparência, voz, modelo ────────────────────────────────────────── */
    function ligarPrefs() {
        const radio = (nome, chave) => {
            const sincronizar = () => $$(`input[name="${nome}"]`).forEach(r => { r.checked = r.value === prefs.get(chave); });
            sincronizar();
            prefs.assinar(chave, sincronizar);
            $$(`input[name="${nome}"]`).forEach(r => r.addEventListener('change', () => { if (r.checked) prefs.set(chave, r.value); }));
        };
        radio('theme', 'theme');
        radio('density', 'density');

        const escala = $('#cfg-scale'), saida = $('#cfg-scale-out');
        const pintaEscala = () => { escala.value = prefs.get('scale'); saida.textContent = `${Math.round(prefs.get('scale') * 100)}%`; };
        pintaEscala();
        escala.addEventListener('input', () => prefs.set('scale', +escala.value));
        prefs.assinar('scale', pintaEscala);

        const mov = $('#cfg-motion');
        mov.value = prefs.get('motion');
        mov.addEventListener('change', () => prefs.set('motion', mov.value));
        prefs.assinar('motion', () => { mov.value = prefs.get('motion'); });

        const tts = $('#cfg-tts');
        const pintaTts = () => { tts.checked = !prefs.get('tts_mudo'); };
        pintaTts();
        prefs.assinar('tts_mudo', pintaTts);
        tts.addEventListener('change', () => { if (tts.checked === prefs.get('tts_mudo')) O.acoes.alternarTts(); });

        const sons = $('#cfg-sounds');
        sons.checked = !!prefs.get('sons');
        sons.addEventListener('change', () => { prefs.set('sons', sons.checked); if (sons.checked) O.som.envio(); });

        const modelo = $('#cfg-model');
        modelo.replaceChildren(...O.composer.MODELOS.map(m => el('option', { value: m.id, text: m.nome })));
        modelo.value = prefs.get('model');
        modelo.addEventListener('change', () => prefs.set('model', modelo.value));
        prefs.assinar('model', () => { modelo.value = prefs.get('model'); });
    }

    /* ── conexão ───────────────────────────────────────────────────────── */
    function ligarConexao() {
        const url = $('#cfg-url'), token = $('#cfg-token'), saida = $('#cfg-test-out');
        url.value = prefs.get('base_url'); token.value = prefs.get('token');
        url.addEventListener('change', () => {
            const v = url.value.trim().replace(/\/+$/, '');
            if (v && !/^https?:\/\/[^\s/]+/i.test(v)) {
                url.setAttribute('aria-invalid', 'true');
                saida.textContent = 'Use um endereço completo, como http://127.0.0.1:8000.';
                return;
            }
            url.removeAttribute('aria-invalid');
            saida.textContent = '';
            prefs.set('base_url', v);
            url.value = v;
            bus.emit('conn:recarregar');
            O.sidebar.carregar();
        });
        token.addEventListener('change', () => prefs.set('token', token.value.trim()));

        $('#cfg-test').addEventListener('click', async e => {
            const b = e.currentTarget;
            b.disabled = true;
            saida.textContent = 'Testando…';
            const r = await api.ping(3000);
            if (!r.ok) { saida.textContent = `Sem resposta de ${api.base()}.`; b.disabled = false; return; }
            let extra = '';
            if (api.token()) {
                try { await api.aprovacoes(); extra = ' · token válido'; }
                catch (err) { extra = err.status === 401 || err.status === 403 ? ' · token recusado' : err.status === 404 ? ' · este cérebro não usa token' : ''; }
            }
            saida.textContent = `Conectado a ${api.base()} em ${r.ms} ms${extra}.`;
            b.disabled = false;
        });
    }

    /* ── atividade ─────────────────────────────────────────────────────── */
    const medidor = (nome, pct) => {
        const sev = CH.severidade(pct);
        const v = pct == null ? null : Math.max(0, Math.min(100, +pct));
        return el('div', { class: 'meter', dataset: { sev }, role: v == null ? 'group' : 'meter', 'aria-label': nome, 'aria-valuemin': v == null ? null : '0', 'aria-valuemax': v == null ? null : '100', 'aria-valuenow': v == null ? null : String(Math.round(v)) },
            el('span', { text: nome }), el('span', { class: 'meter-track' }, el('span', { class: 'meter-fill', style: `width:${v ?? 0}%` })),
            el('span', { class: 'meter-val', text: U.fmtPct(pct) }));
    };

    async function metricas() {
        let m = null;
        try { m = await api.metrics(); } catch (_) { /* painel mostra "—" */ }
        $('#a-meters').replaceChildren(medidor('CPU', m?.cpu_pct), medidor('RAM', m?.ram_pct), medidor('GPU', m?.gpu_pct), medidor('VRAM', m?.vram_pct));
        if (m?.latencia_ms != null) {
            $('#a-latencia').textContent = U.fmtMs(m.latencia_ms);
            latencias.push(m.latencia_ms);
            if (latencias.length > MAX_LAT) latencias.shift();
        } else if (!m) $('#a-latencia').textContent = '—';
        const s = CH.serie(latencias, 200, 48, { margem: 4 });
        const svg = $('#a-spark');
        svg.querySelector('.line').setAttribute('d', s.linha);
        svg.querySelector('.area').setAttribute('d', s.area);
        svg.setAttribute('aria-label', latencias.length > 1 ? `Latência recente: de ${U.fmtMs(s.min)} a ${U.fmtMs(s.max)}` : 'Latência recente: ainda sem pontos suficientes');
    }

    async function cascata() {
        let s = null;
        try { s = await api.stats(); } catch (_) { /* sem estatísticas */ }
        $('#a-conversas').textContent = s ? U.fmtNum(s.total_chats) : '—';
        const lista = $('#a-tiers');
        if (!s?.tiers) { lista.replaceChildren(el('p', { class: 'row-desc', text: api.suporta('stats') ? 'Sem estatísticas do cérebro agora.' : 'Estatísticas ainda indisponíveis neste backend.' })); return; }
        const dist = CH.barras(s.distribuicao_pct || {});
        lista.replaceChildren(...dist.map(d => {
            const t = s.tiers[d.nome] || {};
            return el('div', { class: 'tier-row' }, el('span', { text: d.nome }),
                el('span', { class: 'meter-track', role: 'img', 'aria-label': `${d.nome}: ${d.pct}%` }, el('span', { class: 'meter-fill', style: `width:${d.largura}%` })),
                el('span', { class: 'meter-val', text: `${d.pct}%` }),
                el('small', { text: `${U.fmtNum(t.usos)} usos · ${U.fmtNum(t.falhas)} falhas · ${t.latencia_media_ms != null ? U.fmtMs(t.latencia_media_ms) : '—'}` }));
        }));
    }

    async function servicos() {
        let h = null;
        try { h = await api.health(); } catch (_) { /* sem /health */ }
        const linha = (nome, ok, texto) => el('div', { class: 'row' },
            el('div', { class: 'row-main', style: 'display:flex;align-items:center;gap:.7rem' }, el('span', { class: 'status-dot', dataset: { state: ok ? 'ok' : 'danger' }, 'aria-hidden': 'true' }), el('span', { class: 'row-title', text: nome })),
            el('span', { class: 'mono', text: texto }));
        const caixa = $('#a-servicos');
        if (h?.components) {
            caixa.replaceChildren(linha('Memória', h.components.memory === 'ok', h.components.memory), linha('Modelo', !!h.components.gateway, h.components.gateway ? 'configurado' : 'indisponível'));
            $('#a-vetores').textContent = '—';
            return;
        }
        if (!h) { caixa.replaceChildren(linha('Qdrant', false, 'sem resposta'), linha('SurrealDB', false, 'sem resposta')); $('#a-vetores').textContent = '—'; return; }
        caixa.replaceChildren(
            linha('Qdrant', !!h.qdrant?.ok, h.qdrant?.ok ? U.fmtMs(h.qdrant.latencia_ms) : 'fora do ar'),
            linha('SurrealDB', !!h.surreal?.ok, h.surreal?.ok ? U.fmtMs(h.surreal.latencia_ms) : 'fora do ar'));
        const v = h.qdrant?.vetores ? Object.values(h.qdrant.vetores).reduce((a, x) => a + (+x || 0), 0) : null;
        $('#a-vetores').textContent = v == null ? '—' : U.fmtNum(v);
    }

    const parar = () => { timers.forEach(clearInterval); timers = []; };
    function iniciarAtividade() {
        parar();
        const rodar = (fn, ms) => { fn(); timers.push(setInterval(() => { if (!document.hidden && ativo) fn(); }, ms)); };
        rodar(metricas, 4000); rodar(cascata, 10000); rodar(servicos, 10000);
    }

    /* ── atalhos e sobre ───────────────────────────────────────────────── */
    function montarAtalhos() {
        const partes = [];
        for (const [grupo, itens] of Object.entries(O.atalhos.porGrupo())) {
            const dl = el('dl', { class: 'kbd-grid' });
            for (const a of itens) dl.append(el('dt', { text: a.rotulo }), el('dd', {}, ...a.teclas.map(k => el('kbd', { text: k }))));
            partes.push(el('h4', { class: 'kbd-group', text: grupo }), dl);
        }
        $('#kbd-list').replaceChildren(...partes);
    }

    async function sobre() {
        $('#sobre-front').textContent = `v${O.versao}`;
        $('#sobre-modo').textContent = `${O.desktop() ? 'App desktop (pywebview)' : 'Navegador'} · ${api.base()}`;
        try { const r = api.estado().backend === 'orion' ? { servico: 'Orion', versao: api.estado().app_version } : api.estado().backend === 'legacy' ? await api.req('/', { timeout: 2500 }) : null; $('#sobre-cerebro').textContent = r?.versao ? `${r.servico || 'cérebro'} ${r.versao}` : (r?.servico || 'conectado'); }
        catch (_) { $('#sobre-cerebro').textContent = 'sem resposta'; }
    }

    /* ── navegação interna ─────────────────────────────────────────────── */
    function ligarNavegacao() {
        const nav = $$('.settings-nav a');
        const marcar = id => nav.forEach(a => a.setAttribute('aria-current', String(a.dataset.sec === id)));
        nav.forEach(a => a.addEventListener('click', e => { e.preventDefault(); irPara(a.dataset.sec); }));
        const pagina = $('#view-config');
        const secoes = nav.map(a => $(`#${a.dataset.sec}`));
        let travado = false;
        pagina.addEventListener('scroll', U.debounce(() => {
            if (travado) return;
            const topo = pagina.getBoundingClientRect().top + 90;
            let atual = secoes[0];
            for (const s of secoes) if (s.getBoundingClientRect().top <= topo) atual = s;
            marcar(atual.id);
        }, 60), { passive: true });
        O.views.config.irPara = id => { travado = true; marcar(id); setTimeout(() => { travado = false; }, 700); };
    }
    function irPara(id) {
        const s = document.getElementById(id);
        if (!s) return;
        O.views.config.irPara?.(id);
        s.scrollIntoView({ behavior: O.movimentoReduzido() ? 'auto' : 'smooth', block: 'start' });
        s.setAttribute('tabindex', '-1');
        s.focus({ preventScroll: true });
    }

    O.views = O.views || {};
    O.views.config = {
        init() {
            ligarPrefs(); ligarConexao(); montarAtalhos(); ligarNavegacao();
        },
        ativar(opcoes = {}) {
            ativo = true;
            let aviso = $('#activity-capabilities');
            if (!api.suporta('metrics') && api.estado().api === 'online') {
                if (!aviso) { aviso = el('p', { id: 'activity-capabilities', class: 'banner banner-warn', role: 'status', text: 'Métricas de atividade ainda indisponíveis neste backend.' }); $('#cfg-atividade').append(aviso); }
            } else aviso?.remove();
            iniciarAtividade();
            sobre();
            montarAtalhos();
            if (opcoes.secao) requestAnimationFrame(() => irPara(opcoes.secao));
        },
        desativar() { ativo = false; parar(); },
    };
})();
