/* ==========================================================================
   ORION — legenda_logica.js | lógica pura da legenda ao vivo (E11.7) e do indicador da câmera
   Sem DOM: o que mostrar e o que dizer quando a câmera liga ou desliga. Testável em Node.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else raiz.OrionLegendaLogica = api;
})(typeof self !== 'undefined' ? self : this, function () {
    'use strict';

    /** as últimas `n` falas, mais recente por último; lista inválida vira vazia */
    function ultimasFalas(linhas, n = 3) {
        if (!Array.isArray(linhas)) return [];
        return linhas.filter(l => typeof l === 'string' && l.trim()).map(l => l.trim()).slice(-n);
    }

    /** mensagem de voz ao vivo → estado da câmera; só `camera` muda o indicador */
    function estadoDaCamera(mensagem, atual = false) {
        if (!mensagem || mensagem.type !== 'camera') return atual;
        return mensagem.on === true;
    }

    /** texto do aviso ao ligar ou desligar a câmera (anunciado para leitores de tela) */
    function avisoDaCamera(ligada) {
        return ligada
            ? 'Câmera ligada na voz: um quadro por segundo. Desligue quando quiser.'
            : 'Câmera desligada na voz.';
    }

    return { ultimasFalas, estadoDaCamera, avisoDaCamera };
});
