/* ==========================================================================
   ORION — ponte_pagina.js | as janelas pequenas da ponte de desktop (regra 50)
   Modo `rapido` (E2.3): uma linha vira tarefa/lembrete/gasto/nota por POST /captura.
   Modo `isso` (E2.5): mostra a captura da tela e pergunta ao modelo de visão (POST /ponte/explicar).
   O token da ponte vem no fragmento da URL, fica só em memória e sai da barra de endereços.
   Só `textContent`: nada que vem do servidor vira HTML.
   ========================================================================== */
(function () {
    'use strict';
    const P = window.Orion.ponteLogica;
    const $ = s => document.querySelector(s);

    const token = P.tokenDoFragmento(location.hash);
    history.replaceState(null, '', location.pathname + location.search);
    const modo = P.modoDaBusca(location.search);
    let ultimo = null;       // {id} do que acabou de ser criado (para desfazer)
    let ocupado = false;

    const mostrar = (id, ligado) => { $(id).hidden = !ligado; };
    function erro(texto) { const e = $('#erro'); e.textContent = texto || ''; e.hidden = !texto; }
    function resultado(texto, desfazer) {
        $('#resultado-texto').textContent = texto; mostrar('#resultado', !!texto);
        mostrar('#desfazer', !!desfazer);
    }

    async function chamar(caminho, init = {}) {
        const resp = await fetch(caminho, {
            ...init,
            headers: { ...(init.json !== undefined ? { 'Content-Type': 'application/json' } : {}), Authorization: `Bearer ${token}` },
            body: init.json !== undefined ? JSON.stringify(init.json) : undefined,
        });
        let dado = null;
        try { dado = await resp.json(); } catch (_) { /* corpo vazio */ }
        if (!resp.ok) { const e = new Error(P.erroDaCaptura(resp.status, dado?.detail)); e.status = resp.status; e.detalhe = dado?.detail; throw e; }
        return dado;
    }

    /* ── rápido ────────────────────────────────────────────────────────── */
    async function guardar(ev) {
        ev.preventDefault();
        const campo = $('#rapido-texto'), texto = campo.value.trim();
        if (!texto || ocupado) return;
        ocupado = true; erro(''); campo.disabled = true;
        try {
            const r = await chamar('/captura', { method: 'POST', json: { texto } });
            ultimo = r; campo.value = '';
            resultado(P.resumoDaCaptura(r) + (r.aviso ? ` (${r.aviso})` : ''), true);
        } catch (e) {
            erro(e.status ? e.message : P.erroDaCaptura(0));
        } finally {
            ocupado = false; campo.disabled = false; campo.focus();
        }
    }
    async function desfazer() {
        if (!ultimo || ocupado) return;
        ocupado = true;
        try {
            await chamar(`/captura/${encodeURIComponent(ultimo.id)}`, { method: 'DELETE' });
            resultado('Desfeito.', false); ultimo = null;
        } catch (e) { erro(e.message); }
        finally { ocupado = false; $('#rapido-texto').focus(); }
    }

    /* ── "o que é isso?" (E2.5) ───────────────────────────────────────── */
    let confirmado = false;
    async function carregarImagem() {
        const id = P.imagemDaBusca(location.search);
        if (!id) { erro(P.erroDaExplicacao(404)); return null; }
        try {
            const resp = await fetch(`/ponte/imagem/${encodeURIComponent(id)}`, { headers: { Authorization: `Bearer ${token}` } });
            if (!resp.ok) { erro(P.erroDaExplicacao(resp.status)); return null; }
            const img = $('#isso-imagem');
            img.src = URL.createObjectURL(await resp.blob()); img.hidden = false;
            return id;
        } catch (_) { erro(P.erroDaExplicacao(0)); return null; }
    }
    async function perguntar(ev, id) {
        ev.preventDefault();
        if (ocupado) return;
        ocupado = true; erro(''); resultado('', false); $('#isso-enviar').disabled = true;
        try {
            const r = await chamar('/ponte/explicar', { method: 'POST', json: { imagem_id: id, pergunta: $('#isso-pergunta').value, aceito: confirmado } });
            if (r.aviso === 'primeira_vez') {   // nada saiu: o servidor espera o seu aceite
                confirmado = true; const aviso = $('#isso-aviso'); aviso.textContent = P.AVISO_PRIMEIRA_VEZ; aviso.hidden = false;
                $('#isso-enviar').textContent = 'Entendi, enviar';
            } else {
                $('#isso-aviso').hidden = true;
                resultado(r.texto || 'O modelo não devolveu texto.', false);
            }
        } catch (e) {
            erro(P.erroDaExplicacao(e.status, e.detalhe));
        } finally { ocupado = false; $('#isso-enviar').disabled = false; }
    }

    /* ── início ────────────────────────────────────────────────────────── */
    function fechar() { try { window.close(); } catch (_) { /* a ponte fecha a janela */ } }
    document.addEventListener('keydown', e => { if (e.key === 'Escape') fechar(); });
    if (!token) { erro(P.erroDaCaptura(403)); return; }
    if (modo === 'rapido') {
        mostrar('#rapido', true);
        $('#rapido').addEventListener('submit', guardar);
        $('#desfazer').addEventListener('click', desfazer);
        $('#rapido-texto').focus();
    }
    if (modo === 'isso') {
        mostrar('#isso', true);
        carregarImagem().then(id => { if (id) { $('#isso-form').addEventListener('submit', ev => perguntar(ev, id)); $('#isso-pergunta').focus(); $('#isso-pergunta').select(); } else $('#isso-enviar').disabled = true; });
    }
    window.Orion.ponte = { fechar };
})();
