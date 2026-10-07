/* ==========================================================================
   ORION — fala_core.js | partes puras da voz por clique (sem DOM, testáveis em Node)
   Protocolo do /ws/voz (JSON do servidor):
     heard {text} · ev {ev: corpo do /chat} · audio {mime} + bytes · error {msg} · done
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).falaCore = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
    'use strict';

    // o servidor reconhece webm, ogg e mp4 pelo cabeçalho; a ordem é a de preferência
    const MIMES = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus', 'audio/mp4'];

    /** primeiro formato que o navegador sabe gravar, ou null (sem MediaRecorder) */
    function escolherMime(suporta) {
        for (const m of MIMES) { try { if (suporta(m)) return m; } catch (_) { /* navegador sem a checagem */ } }
        return null;
    }

    /**
     * Mensagem JSON do servidor → eventos internos do chat (os mesmos do SSE).
     * `normalizar` é o O.sse.normalizar. `audio` não gera evento: o próximo quadro binário é a fala.
     */
    function eventosDaMensagem(m, normalizar) {
        if (!m || typeof m !== 'object') return [];
        switch (m.type) {
            case 'heard':
                return typeof m.text === 'string' && m.text.trim()
                    ? [{ tipo: 'inicio' }, { tipo: 'usuario', texto: m.text.trim() }] : [];
            case 'ev': { const ev = normalizar(m.ev); return ev && ev.tipo !== 'fim' ? [ev] : []; }
            case 'error': return [{ tipo: 'erro', mensagem: String(m.msg || 'Algo deu errado com a voz.') }];
            case 'done': return [{ tipo: 'fim' }];
            default: return [];
        }
    }

    /** o token (modo máquina) vai na primeira mensagem, nunca na URL; com cookie não precisa */
    const mensagemDeAuth = token => (token ? JSON.stringify({ cmd: 'auth', token }) : null);

    return { MIMES, escolherMime, eventosDaMensagem, mensagemDeAuth };
});
