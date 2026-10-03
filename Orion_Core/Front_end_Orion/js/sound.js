/* ==========================================================================
   ORION — sound.js | tons sintetizados (Web Audio), sem arquivos
   O webview bloqueia áudio antes do primeiro gesto: o tom de boot pode não tocar na
   primeira abertura. Esperado. Desligável em Configurações › Voz e sons.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    let ctx = null;

    let gesto = false;     // criar o contexto antes do primeiro gesto gera aviso no console e não toca mesmo
    const obter = () => {
        if (!gesto) return null;
        if (!ctx) { try { ctx = new (window.AudioContext || window.webkitAudioContext)(); } catch (_) { ctx = null; } }
        return ctx;
    };
    const acordar = () => { gesto = true; obter()?.resume?.(); };
    document.addEventListener('pointerdown', acordar, { once: true });
    document.addEventListener('keydown', acordar, { once: true });

    function tom({ freq = 440, ate = null, dur = 0.3, tipo = 'sine', vol = 0.04, atraso = 0 }) {
        if (!O.prefs.get('sons')) return;
        const c = obter();
        if (!c || c.state === 'suspended') return;
        const t0 = c.currentTime + atraso;
        const osc = c.createOscillator(), g = c.createGain();
        osc.type = tipo;
        osc.frequency.setValueAtTime(freq, t0);
        if (ate) osc.frequency.exponentialRampToValueAtTime(ate, t0 + dur);
        g.gain.setValueAtTime(0.0001, t0);
        g.gain.linearRampToValueAtTime(vol, t0 + 0.02);
        g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
        osc.connect(g).connect(c.destination);
        osc.start(t0);
        osc.stop(t0 + dur + 0.05);
    }

    O.som = {
        boot() { tom({ freq: 146.8, dur: 1.1, vol: 0.05 }); tom({ freq: 220, dur: 0.9, vol: 0.03, atraso: 0.14 }); },
        mensagem() { tom({ freq: 660, ate: 880, dur: 0.14, vol: 0.026 }); },
        envio() { tom({ freq: 520, dur: 0.07, vol: 0.02 }); },
        atencao() { tom({ freq: 740, dur: 0.12, vol: 0.03 }); tom({ freq: 740, dur: 0.12, vol: 0.03, atraso: 0.18 }); },
        erro() { tom({ freq: 330, ate: 220, dur: 0.22, vol: 0.03, tipo: 'triangle' }); },
    };
})();
