/* ==========================================================================
   ORION — util.js | funções puras (sem DOM): texto, datas, formatação, timing
   UMD: no navegador publica `Orion.util`; em Node, `require('./util.js')`.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else { (raiz.Orion = raiz.Orion || {}).util = api; raiz._escapar = api.escapar; }
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    const ENTIDADES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
    const escapar = s => String(s).replace(/[&<>"']/g, c => ENTIDADES[c]);
    const desescapar = s => String(s)
        .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&');

    /** minúsculas e sem acento: "Memória" → "memoria" (busca e ordenação tolerantes) */
    const norm = s => String(s).normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();

    const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

    function hostDe(url) {
        try { return new URL(desescapar(url)).hostname.replace(/^www\./, ''); }
        catch (_) { return ''; }
    }

    function truncar(s, n) {
        s = String(s ?? '');
        return s.length <= n ? s : s.slice(0, Math.max(0, n - 1)).trimEnd() + '…';
    }

    /* ── datas ─────────────────────────────────────────────────────────── */
    const inicioDoDia = d => { const x = new Date(d); x.setHours(0, 0, 0, 0); return x; };
    const diasEntre = (a, b) => Math.round((inicioDoDia(a) - inicioDoDia(b)) / 86400000);
    const MESES = ['janeiro', 'fevereiro', 'março', 'abril', 'maio', 'junho', 'julho', 'agosto',
                   'setembro', 'outubro', 'novembro', 'dezembro'];

    function dataValida(v) {
        if (v == null || v === '') return null;
        const d = v instanceof Date ? v : new Date(v);
        return Number.isNaN(d.getTime()) ? null : d;
    }

    /** rótulo de grupo para a lista de conversas e separadores do chat */
    function rotuloDia(valor, agora = new Date()) {
        const d = dataValida(valor);
        if (!d) return 'Sem data';
        const dif = diasEntre(agora, d);
        if (dif <= 0) return 'Hoje';
        if (dif === 1) return 'Ontem';
        if (dif < 7) return 'Esta semana';
        if (dif < 30) return 'Este mês';
        const m = MESES[d.getMonth()];
        return d.getFullYear() === agora.getFullYear() ? m[0].toUpperCase() + m.slice(1)
                                                         : `${m[0].toUpperCase() + m.slice(1)} de ${d.getFullYear()}`;
    }

    /** agrupa mantendo a ordem: [{rotulo, itens}] */
    function agruparPorDia(itens, pegarData, agora = new Date()) {
        const grupos = [];
        for (const it of itens) {
            const rotulo = rotuloDia(pegarData(it), agora);
            const g = grupos[grupos.length - 1];
            if (g && g.rotulo === rotulo) g.itens.push(it); else grupos.push({ rotulo, itens: [it] });
        }
        return grupos;
    }

    const hora = d => {
        const x = dataValida(d);
        return x ? `${String(x.getHours()).padStart(2, '0')}:${String(x.getMinutes()).padStart(2, '0')}` : '';
    };

    function dataCurta(d) {
        const x = dataValida(d);
        return x ? `${String(x.getDate()).padStart(2, '0')}/${String(x.getMonth() + 1).padStart(2, '0')}` : '';
    }

    function quando(valor, agora = new Date()) {
        const d = dataValida(valor);
        if (!d) return '';
        const seg = Math.round((agora - d) / 1000);
        if (seg < 45) return 'agora';
        if (seg < 3600) return `há ${Math.max(1, Math.round(seg / 60))} min`;
        const dif = diasEntre(agora, d);
        if (dif <= 0) return `há ${Math.round(seg / 3600)} h`;
        if (dif === 1) return 'ontem';
        return dataCurta(d);
    }

    function saudacao(agora = new Date()) {
        const h = agora.getHours();
        if (h < 5) return 'Boa madrugada';
        if (h < 12) return 'Bom dia';
        if (h < 18) return 'Boa tarde';
        return 'Boa noite';
    }

    /* ── números ───────────────────────────────────────────────────────── */
    const fmtMs = v => (v == null || Number.isNaN(+v)) ? '—' : +v >= 10000 ? `${(+v / 1000).toFixed(1)} s` : `${Math.round(+v)} ms`;
    const fmtPct = v => (v == null || Number.isNaN(+v)) ? '—' : `${Math.round(+v)}%`;
    function fmtNum(v) {
        if (v == null || Number.isNaN(+v)) return '—';
        return new Intl.NumberFormat('pt-BR').format(+v);
    }
    function fmtBytes(n) {
        n = +n;
        if (!Number.isFinite(n)) return '—';
        const un = ['B', 'KB', 'MB', 'GB'];
        let i = 0;
        while (n >= 1024 && i < un.length - 1) { n /= 1024; i++; }
        return `${n >= 100 || i === 0 ? Math.round(n) : n.toFixed(1)} ${un[i]}`;
    }

    /* ── timing ────────────────────────────────────────────────────────── */
    function debounce(fn, ms) {
        let t = null;
        const d = (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
        d.cancel = () => clearTimeout(t);
        return d;
    }

    /** agrupa chamadas no próximo quadro (streaming: 1 render por quadro, não por pedaço) */
    function noProximoQuadro(fn, raf = (typeof requestAnimationFrame === 'function' ? requestAnimationFrame : cb => setTimeout(cb, 16))) {
        let agendado = false;
        const f = () => {
            if (agendado) return;
            agendado = true;
            raf(() => { agendado = false; fn(); });
        };
        return f;
    }

    let _seq = 0;
    const uid = (p = 'id') => `${p}-${Date.now().toString(36)}-${(++_seq).toString(36)}`;

    return {
        escapar, desescapar, norm, clamp, hostDe, truncar,
        rotuloDia, agruparPorDia, hora, dataCurta, quando, saudacao, dataValida, diasEntre,
        fmtMs, fmtPct, fmtNum, fmtBytes, debounce, noProximoQuadro, uid,
    };
});
