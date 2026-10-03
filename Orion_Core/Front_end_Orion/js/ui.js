/* ==========================================================================
   ORION — ui.js | toasts, diálogos de confirmação, menus, copiar
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, el, icone, anunciar } = O;

    /* ── toasts ────────────────────────────────────────────────────────── */
    const ICONE_TOAST = { erro: 'alert', aviso: 'alert', ok: 'check', info: 'info' };
    const MAX_TOASTS = 3;

    /**
     * @param {string} msg
     * @param {{tipo?: 'info'|'ok'|'aviso'|'erro', ms?: number, acao?: {rotulo: string, fn: Function}, id?: string}} [o]
     * `id` substitui o toast anterior com o mesmo id (ex.: "reconectando" não empilha).
     */
    function toast(msg, { tipo = 'info', ms = 4200, acao = null, id = null } = {}) {
        const raiz = $('#toasts');
        if (!raiz) return () => {};
        if (id) raiz.querySelectorAll(`[data-id="${CSS.escape(id)}"]`).forEach(t => fechar(t, true));
        while (raiz.children.length >= MAX_TOASTS) fechar(raiz.firstElementChild, true);
        const t = el('div', { class: 'toast', role: tipo === 'erro' ? 'alert' : 'status', dataset: { tipo, ...(id ? { id } : {}) } },
            el('span', { html: icone(ICONE_TOAST[tipo] || 'info', 'toast-icon') }),
            el('span', { class: 'toast-msg', text: msg }));
        if (acao) {
            t.append(el('button', { class: 'btn btn-ghost btn-sm', type: 'button', text: acao.rotulo,
                on: { click: () => { fechar(t); acao.fn(); } } }));
        }
        t.append(el('button', { class: 'icon-btn', type: 'button', 'aria-label': 'Dispensar notificação', html: icone('close'),
            on: { click: () => fechar(t) } }));
        raiz.append(t);
        requestAnimationFrame(() => requestAnimationFrame(() => { t.dataset.show = 'true'; }));
        let tid = null;
        const armar = () => { if (ms > 0) tid = setTimeout(() => fechar(t), ms); };
        t.addEventListener('mouseenter', () => clearTimeout(tid));
        t.addEventListener('mouseleave', armar);
        t.addEventListener('focusin', () => clearTimeout(tid));
        armar();
        return () => fechar(t);
    }

    function fechar(t, rapido = false) {
        if (!t || t.dataset.saindo) return;
        t.dataset.saindo = '1';
        t.dataset.show = 'false';
        setTimeout(() => t.remove(), rapido ? 0 : 260);
    }

    /* ── diálogo de confirmação (substitui confirm()) ──────────────────── */
    const FOCAVEIS = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

    function prenderFoco(caixa, aoEsc) {
        const tecla = e => {
            if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); aoEsc(); return; }
            if (e.key !== 'Tab') return;
            const itens = Array.from(caixa.querySelectorAll(FOCAVEIS)).filter(x => x.offsetParent !== null);
            if (!itens.length) return;
            const [a, z] = [itens[0], itens[itens.length - 1]];
            if (e.shiftKey && document.activeElement === a) { e.preventDefault(); z.focus(); }
            else if (!e.shiftKey && document.activeElement === z) { e.preventDefault(); a.focus(); }
        };
        document.addEventListener('keydown', tecla, true);
        return () => document.removeEventListener('keydown', tecla, true);
    }

    /** @returns {Promise<boolean>} */
    function confirmar({ titulo, texto, ok = 'Confirmar', cancelar = 'Cancelar', perigo = false }) {
        return new Promise(resolver => {
            const raiz = $('#dialog-root');
            const anterior = document.activeElement;
            const idT = O.util.uid('dlg-t');
            const btnOk = el('button', { class: `btn ${perigo ? 'btn-danger' : 'btn-primary'}`, type: 'button', text: ok });
            const btnNo = el('button', { class: 'btn btn-ghost', type: 'button', text: cancelar });
            const dialogo = el('div', { class: 'dialog', role: 'alertdialog', 'aria-modal': 'true', 'aria-labelledby': idT, style: 'max-width:26rem' },
                el('div', { class: 'dialog-head' }, el('h2', { id: idT, text: titulo })),
                el('div', { class: 'dialog-body', text: texto }),
                el('div', { class: 'dialog-foot' }, btnNo, btnOk));
            const scrim = el('div', { class: 'dialog-scrim center', dataset: { open: 'false' } }, dialogo);
            raiz.append(scrim);
            const soltar = prenderFoco(dialogo, () => fim(false));
            const fim = valor => {
                soltar();
                scrim.dataset.open = 'false';
                setTimeout(() => scrim.remove(), 200);
                anterior?.focus?.();
                resolver(valor);
            };
            btnOk.addEventListener('click', () => fim(true));
            btnNo.addEventListener('click', () => fim(false));
            scrim.addEventListener('mousedown', e => { if (e.target === scrim) fim(false); });
            requestAnimationFrame(() => { scrim.dataset.open = 'true'; (perigo ? btnNo : btnOk).focus(); });
        });
    }

    /* ── menu de botão (ex.: modelo) ───────────────────────────────────── */
    function ligarMenu(botao, menu, { aoEscolher, aoAbrir } = {}) {
        const itens = () => Array.from(menu.querySelectorAll('[role^="menuitem"]'));
        const aberto = () => menu.dataset.open === 'true';
        const abrir = () => {
            aoAbrir?.();
            menu.dataset.open = 'true';
            botao.setAttribute('aria-expanded', 'true');
            (itens().find(i => i.getAttribute('aria-checked') === 'true') || itens()[0])?.focus();
        };
        const fecharMenu = (devolverFoco = true) => {
            if (!aberto()) return;
            menu.dataset.open = 'false';
            botao.setAttribute('aria-expanded', 'false');
            if (devolverFoco) botao.focus();
        };
        botao.addEventListener('click', () => (aberto() ? fecharMenu() : abrir()));
        botao.addEventListener('keydown', e => { if (e.key === 'ArrowUp' || e.key === 'ArrowDown') { e.preventDefault(); if (!aberto()) abrir(); } });
        menu.addEventListener('keydown', e => {
            const lista = itens(), i = lista.indexOf(document.activeElement);
            if (e.key === 'ArrowDown') { e.preventDefault(); lista[(i + 1) % lista.length].focus(); }
            else if (e.key === 'ArrowUp') { e.preventDefault(); lista[(i - 1 + lista.length) % lista.length].focus(); }
            else if (e.key === 'Home') { e.preventDefault(); lista[0].focus(); }
            else if (e.key === 'End') { e.preventDefault(); lista[lista.length - 1].focus(); }
            else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); fecharMenu(); }
            else if (e.key === 'Tab') fecharMenu(false);
        });
        menu.addEventListener('click', e => {
            const it = e.target.closest('[role^="menuitem"]');
            if (!it) return;
            aoEscolher?.(it.dataset.valor);
            fecharMenu();
        });
        document.addEventListener('pointerdown', e => { if (aberto() && !menu.contains(e.target) && !botao.contains(e.target)) fecharMenu(false); });
        return { abrir, fechar: fecharMenu };
    }

    /* ── copiar ────────────────────────────────────────────────────────── */
    async function copiar(texto) {
        try { await navigator.clipboard.writeText(texto); return true; }
        catch (_) {
            // webview sem permissão de clipboard: execCommand ainda funciona em gesto do usuário
            const t = el('textarea', { style: 'position:fixed;opacity:0;top:0', 'aria-hidden': 'true' });
            t.value = texto;
            document.body.append(t);
            t.select();
            let ok = false;
            try { ok = document.execCommand('copy'); } catch (_) { ok = false; }
            t.remove();
            return ok;
        }
    }

    /** feedback curto num botão: "Copiar" → "Copiado" */
    function piscarOk(botao, textoOk, ms = 1400) {
        const antes = botao.textContent;
        botao.classList.add('ok');
        if (textoOk) botao.textContent = textoOk;
        setTimeout(() => { botao.classList.remove('ok'); if (textoOk) botao.textContent = antes; }, ms);
    }

    O.ui = { toast, confirmar, ligarMenu, copiar, piscarOk, prenderFoco, anunciar };
})();
