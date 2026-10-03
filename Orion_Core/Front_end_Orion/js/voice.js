/* ==========================================================================
   ORION — voice.js | voz ao vivo (Gemini Live) via /ws/voice
   Independe do hub e do mic_engine. Microfone exige contexto seguro (HTTPS ou
   localhost): em http://IP-do-tailscale o navegador nem expõe `mediaDevices`.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { bus, $$, ui } = O;

    let ws = null, stream = null, ctx = null, fonte = null, proc = null;
    let ctxPlay = null, proximo = 0, ativa = false;

    const botoes = () => $$('.voice-live-btn');
    const marcar = on => botoes().forEach(b => { b.classList.toggle('active', on); b.setAttribute('aria-pressed', String(on)); });
    const nota = (msg, tipo = 'aviso') => ui.toast(msg, { tipo, ms: 5200 });

    function contextoSaida() {
        if (!ctxPlay) { try { ctxPlay = new (window.AudioContext || window.webkitAudioContext)(); } catch (_) { ctxPlay = null; } }
        return ctxPlay;
    }

    function tocar(buf) {
        const c = contextoSaida();
        if (!c || !buf.byteLength) return;
        const i16 = new Int16Array(buf), f32 = new Float32Array(i16.length);
        let pico = 0;
        for (let i = 0; i < i16.length; i++) { f32[i] = i16[i] / 32768; pico = Math.max(pico, Math.abs(f32[i])); }
        const ab = c.createBuffer(1, f32.length, 24000);
        ab.getChannelData(0).set(f32);
        const src = c.createBufferSource();
        src.buffer = ab;
        src.connect(c.destination);
        const ini = Math.max(c.currentTime, proximo);
        src.start(ini);
        proximo = ini + ab.duration;
        O.estado.definir('speaking');
        bus.emit('audio', Math.min(1, pico * 1.6));
    }

    const cmd = c => { if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ cmd: c })); };

    async function iniciar() {
        if (ativa) return;
        if (!navigator.mediaDevices?.getUserMedia) {
            nota('Microfone indisponível neste endereço: o navegador só libera com HTTPS ou localhost.');
            return;
        }
        ativa = true;
        marcar(true);
        try {
            stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        } catch (e) {
            nota(`Sem acesso ao microfone: ${e.message}`, 'erro');
            ativa = false; marcar(false);
            return;
        }
        ctx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 16000 });
        fonte = ctx.createMediaStreamSource(stream);
        proc = ctx.createScriptProcessor(4096, 1, 1);
        const mudo = ctx.createGain();
        mudo.gain.value = 0;
        fonte.connect(proc); proc.connect(mudo); mudo.connect(ctx.destination);
        proc.onaudioprocess = e => {
            if (ws?.readyState !== WebSocket.OPEN) return;
            const f32 = e.inputBuffer.getChannelData(0), i16 = new Int16Array(f32.length);
            let pico = 0;
            for (let i = 0; i < f32.length; i++) {
                const s = Math.max(-1, Math.min(1, f32[i]));
                i16[i] = s < 0 ? s * 32768 : s * 32767;
                pico = Math.max(pico, Math.abs(s));
            }
            if (O.estado.atual === 'listening') bus.emit('audio', Math.min(1, pico * 2));
            ws.send(i16.buffer);
        };

        ws = new WebSocket(`${O.api.wsBase()}/ws/voice`);
        ws.binaryType = 'arraybuffer';
        ws.onopen = () => { cmd('start'); O.estado.definir('listening'); O.anunciar('Voz ao vivo ligada. Pode falar.'); };
        ws.onmessage = ({ data }) => {
            if (data instanceof ArrayBuffer) { tocar(data); return; }
            let m;
            try { m = JSON.parse(data); } catch (_) { return; }
            if (m.type === 'text' && m.text) bus.emit('chat:evento', { tipo: 'texto', texto: m.text });
            if (m.type === 'done') { bus.emit('chat:evento', { tipo: 'fim' }); if (ativa) O.estado.definir('listening'); }
            if (m.type === 'error') { nota(`Voz ao vivo: ${m.msg}`, 'erro'); parar(); }
        };
        ws.onerror = () => { nota('Voz ao vivo: falha na conexão com o cérebro.', 'erro'); parar(); };
        ws.onclose = () => { if (ativa) parar(); };
    }

    function parar() {
        if (!ativa) return;
        ativa = false;
        marcar(false);
        cmd('stop');
        try { ws?.close(); } catch (_) { /* já fechado */ }
        ws = null;
        stream?.getTracks().forEach(t => t.stop());
        stream = null;
        try { proc?.disconnect(); fonte?.disconnect(); } catch (_) { /* nós já soltos */ }
        try { ctx?.close(); } catch (_) { /* já fechado */ }
        ctx = fonte = proc = null;
        proximo = 0;
        bus.emit('chat:evento', { tipo: 'fim' });
        O.estado.definir('idle');
        O.anunciar('Voz ao vivo desligada.');
    }

    function alternar() { (ativa ? parar() : iniciar()); }
    function ligar() { document.addEventListener('click', e => { if (e.target.closest('.voice-live-btn')) alternar(); }); }

    O.voz = { ligar, iniciar, parar, alternar, ativa: () => ativa };
})();
