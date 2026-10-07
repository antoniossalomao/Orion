/* ==========================================================================
   ORION — fala.js | falar por clique: grava, manda ao /ws/voz e toca a resposta
   Clique no microfone começa a gravar; outro clique (ou Enter/Espaço no botão) envia; Esc descarta.
   A fala vira um turno comum do agente: memória, ferramentas e política valem como no texto.
   Aprovar uma ação NÃO se faz por voz: a resposta fala o aviso e o cartão aparece no chat.
   Microfone exige contexto seguro (HTTPS ou localhost).
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { bus, $$, ui, api } = O;
    const Core = O.falaCore;

    let ws = null, rec = null, stream = null, pedacos = [], gravando = false, esperando = false;
    let proximoEhAudio = false, tocando = null, ignorando = false;

    const botoes = () => $$('.voice-ptt-btn');
    const ocupada = () => gravando || esperando || !!tocando;
    function marcar() {
        const parar = !gravando && (esperando || !!tocando);   // clicar agora para a resposta, não grava
        botoes().forEach(b => {
            b.classList.toggle('active', gravando);
            b.setAttribute('aria-pressed', String(gravando));
            b.dataset.tip = gravando ? 'Parar e enviar (Esc descarta)' : parar ? 'Parar a resposta (Esc)' : 'Falar (clique para gravar)';
            b.setAttribute('aria-label', gravando ? 'Parar e enviar a fala' : parar ? 'Parar a resposta' : 'Falar com o Orion');
            b.dataset.parar = String(parar);
        });
    }
    const nota = (msg, tipo = 'aviso') => ui.toast(msg, { tipo, ms: 5200 });

    function soltarMicrofone() {
        stream?.getTracks().forEach(t => t.stop());
        stream = null; rec = null; pedacos = [];
    }

    function pararAudio() {
        if (tocando) { try { tocando.pause(); URL.revokeObjectURL(tocando.src); } catch (_) { /* já solto */ } tocando = null; marcar(); }
    }
    function tocar(bytes, mime) {
        pararAudio();
        const url = URL.createObjectURL(new Blob([bytes], { type: mime || 'audio/mpeg' }));
        const a = new Audio(url);
        tocando = a;
        a.onended = a.onerror = () => { URL.revokeObjectURL(url); if (tocando === a) { tocando = null; marcar(); } O.estado.definir('idle'); };
        O.estado.definir('speaking');
        marcar();
        a.play().catch(() => { /* o navegador pode barrar áudio sem gesto: o texto já está na tela */ });
    }

    function fecharSocket() { try { ws?.close(); } catch (_) { /* já fechado */ } ws = null; }

    /** abre (ou reaproveita) o WebSocket; resolve quando o servidor pode receber a fala */
    function conectar() {
        if (ws && ws.readyState === WebSocket.OPEN) return Promise.resolve(ws);
        return new Promise((resolve, reject) => {
            const s = new WebSocket(`${api.wsBase()}/ws/voz`);
            s.binaryType = 'arraybuffer';
            s.onopen = () => {
                const auth = Core.mensagemDeAuth(api.token());
                if (auth) s.send(auth);
                ws = s; resolve(s);
            };
            s.onerror = () => reject(new Error('sem conexão com o Orion'));
            s.onclose = () => { if (ws === s) ws = null; if (esperando) { esperando = false; marcar(); bus.emit('chat:evento', { tipo: 'fim' }); } };
            s.onmessage = ({ data }) => {
                if (data instanceof ArrayBuffer) { if (proximoEhAudio && !ignorando) { proximoEhAudio = false; tocar(data, 'audio/mpeg'); } return; }
                let m;
                try { m = JSON.parse(data); } catch (_) { return; }
                if (ignorando) { if (m.type === 'done') ignorando = false; return; }   // resto de um turno cancelado
                if (m.type === 'audio') { proximoEhAudio = true; return; }
                if (m.type === 'heard' && O.app?.ir) O.app.ir('chat', { foco: false });
                Core.eventosDaMensagem(m, O.sse.normalizar).forEach(ev => bus.emit('chat:evento', ev));
                if (m.type === 'done') { esperando = false; marcar(); if (!tocando) O.estado.definir('idle'); }
            };
        });
    }

    async function iniciar() {
        if (gravando || esperando) return;
        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
            nota('Microfone indisponível neste endereço: o navegador só libera com HTTPS ou localhost.');
            return;
        }
        const mime = Core.escolherMime(m => MediaRecorder.isTypeSupported(m));
        if (!mime) { nota('Este navegador não grava áudio em um formato que o Orion entende.', 'erro'); return; }
        pararAudio();
        try { stream = await navigator.mediaDevices.getUserMedia({ audio: true }); }
        catch (e) { nota(`Sem acesso ao microfone: ${e.message}`, 'erro'); return; }
        pedacos = [];
        rec = new MediaRecorder(stream, { mimeType: mime });
        rec.ondataavailable = e => { if (e.data?.size) pedacos.push(e.data); };
        rec.start();
        gravando = true;
        O.estado.definir('listening');
        O.anunciar('Gravando. Clique de novo para enviar.');
        marcar();
        conectar().catch(() => { /* o erro aparece ao enviar */ });
    }

    async function enviar() {
        if (!gravando || !rec) return;
        const r = rec;
        const fim = new Promise(res => { r.onstop = res; });
        r.stop();
        await fim;
        gravando = false;
        const blob = new Blob(pedacos, { type: r.mimeType });
        soltarMicrofone();
        if (blob.size < 1200) { O.estado.definir('idle'); marcar(); nota('Não ouvi nada. Tente de novo.'); return; }
        esperando = true; marcar();
        O.estado.definir('processing');
        try { (await conectar()).send(await blob.arrayBuffer()); }
        catch (e) { esperando = false; marcar(); O.estado.definir('idle'); nota(`Voz: ${e.message}. A voz precisa de ORION_VOICE_ENABLED.`, 'erro'); }
    }

    function descartar() {
        if (!gravando) return false;
        gravando = false;
        try { rec?.stop(); } catch (_) { /* já parado */ }
        soltarMicrofone();
        O.estado.definir('idle'); marcar();
        O.anunciar('Gravação descartada.');
        return true;
    }

    /** para a fala e, se o turno ainda roda, cancela no servidor (o resto que chegar é descartado até o `done`) */
    function cancelar() {
        pararAudio();
        if (esperando) {
            ignorando = true;
            if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ cmd: 'cancel' }));
            else ignorando = false;
            esperando = false;
            bus.emit('chat:evento', { tipo: 'fim', interrompida: true });
        }
        if (!gravando) O.estado.definir('idle');
        marcar();
        O.anunciar('Resposta interrompida.');
    }

    const alternar = () => (gravando ? enviar() : ocupada() ? cancelar() : iniciar());
    function ligar() {
        document.addEventListener('click', e => { if (e.target.closest('.voice-ptt-btn')) alternar(); });
        // captura para ganhar do Esc do chat (parar resposta) só quando há gravação em curso
        document.addEventListener('keydown', e => {
            if (e.key !== 'Escape') return;
            if (descartar() || (tocando && (cancelar(), true))) { e.preventDefault(); e.stopPropagation(); }
        }, true);
        marcar();
    }

    O.fala = { ligar, alternar, descartar, cancelar, gravando: () => gravando, ocupada: () => esperando || !!tocando };
})();
