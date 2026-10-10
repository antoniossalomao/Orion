/* ==========================================================================
   ORION — voice.js | voz ao vivo (Gemini Live) via /ws/voice
   Captura por AudioWorklet (js/voice-worklet.js). Independe do hub e do mic_engine.
   Microfone exige contexto seguro (HTTPS ou localhost): em http://IP-do-tailscale o navegador nem expõe `mediaDevices`.
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
        if (!O.api.suporta('voice')) { nota('Voz ao vivo ainda indisponível neste backend.'); return; }
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
        const mudo = ctx.createGain();
        mudo.gain.value = 0;
        const enviarPcm = f32 => {
            if (ws?.readyState !== WebSocket.OPEN) return;
            const { pcm, pico } = O.falaCore.floatParaPcm16(f32);
            if (O.estado.atual === 'listening') bus.emit('audio', Math.min(1, pico * 2));
            ws.send(pcm.buffer);
        };
        try {
            await ctx.audioWorklet.addModule('js/voice-worklet.js');
            proc = new AudioWorkletNode(ctx, 'captura-orion');
            proc.port.onmessage = e => enviarPcm(e.data);
            fonte.connect(proc); proc.connect(mudo); mudo.connect(ctx.destination);
        } catch (_) {
            // navegador sem AudioWorklet (ou módulo bloqueado): o ScriptProcessor, obsoleto, ainda serve
            proc = ctx.createScriptProcessor(4096, 1, 1);
            fonte.connect(proc); proc.connect(mudo); mudo.connect(ctx.destination);
            proc.onaudioprocess = e => enviarPcm(e.inputBuffer.getChannelData(0));
        }

        ws = new WebSocket(`${O.api.wsBase()}/ws/voice`);
        ws.binaryType = 'arraybuffer';
        ws.onopen = () => { const t = O.api.token(); if (t) ws.send(JSON.stringify({ cmd: 'auth', token: t })); cmd('start'); O.estado.definir('listening'); O.anunciar('Voz ao vivo ligada. Pode falar.'); };
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
