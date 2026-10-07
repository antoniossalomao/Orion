/* ==========================================================================
   ORION — transport.js | como o chat chega ao cérebro e como a resposta volta
   Dois caminhos, MESMOS eventos internos (`chat:evento` no bus):
     · SSE  — fetch em streaming no /chat (navegador, ou desktop sem o hub);
     · hub  — pywebview.process_command + WebSocket :8765 (desktop; mantém o
              mic_engine e o resto do ecossistema enxergando a conversa).
   Eventos: inicio · modelo · texto · ferramenta · aprovacao · erro · fim
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { bus, api } = O;

    const emitir = ev => bus.emit('chat:evento', ev);

    /* ── SSE ───────────────────────────────────────────────────────────── */
    let abortar = null;      // cancela o stream em curso
    let ignorarHub = false;  // "parar" no caminho do hub: descarta o resto até o idle

    async function lerStream(caminho, init, rotuloFalha) {
        const ctrl = new AbortController();
        abortar = () => ctrl.abort();
        emitir({ tipo: 'inicio' });
        let interrompida = false;
        try {
            const token = api.token();
            const headers = { Accept: 'text/event-stream', ...(init.json !== undefined ? { 'Content-Type': 'application/json' } : {}) };
            if (token) headers.Authorization = `Bearer ${token}`;
            const resp = await fetch(api.base() + caminho, {
                method: 'POST', headers, signal: ctrl.signal,
                body: init.json !== undefined ? JSON.stringify(init.json) : undefined,
            });
            if (!resp.ok) {
                const corpo = await resp.text().catch(() => '');
                let msg = `${rotuloFalha} (HTTP ${resp.status}).`;
                if (resp.status === 401 || resp.status === 403) msg = 'Acesso negado: confira o token em Configurações › Conexão.';
                else { try { const d = JSON.parse(corpo).detail; if (typeof d === 'string') msg = d; } catch (_) { /* corpo não-JSON */ } }
                bus.emit('chat:recusado', { caminho, pedido: init.json, status: resp.status, mensagem: msg });
                emitir({ tipo: 'erro', mensagem: msg, status: resp.status });
                return;
            }
            if (!resp.body) throw new Error('sem corpo de resposta');
            const parser = O.sse.criarParser(bruto => {
                const ev = O.sse.normalizar(bruto);
                if (ev && ev.tipo !== 'fim') emitir(ev);
            });
            const leitor = resp.body.getReader();
            const dec = new TextDecoder();
            for (;;) {
                const { done, value } = await leitor.read();
                if (done) break;
                parser.alimentar(dec.decode(value, { stream: true }));
            }
            parser.fim();
        } catch (e) {
            if (e.name === 'AbortError') interrompida = true;
            else emitir({ tipo: 'erro', mensagem: 'A conexão com o cérebro caiu no meio da resposta.', rede: true });
        } finally {
            abortar = null;
            emitir({ tipo: 'fim', interrompida });
        }
    }

    /* ── hub (desktop) ─────────────────────────────────────────────────── */
    let hub = null, hubAberto = false, espera = 2000, hubQuer = false;

    function conectarHub() {
        hubQuer = true;
        if (hub || !('WebSocket' in window)) return;
        let ws;
        try { ws = new WebSocket('ws://127.0.0.1:8765'); } catch (_) { return agendarHub(); }
        hub = ws;
        ws.onopen = () => { hubAberto = true; espera = 2000; bus.emit('hub', true); };
        ws.onmessage = ({ data }) => {
            let m;
            try { m = JSON.parse(data); } catch (_) { return; }
            tratarHub(m);
        };
        ws.onclose = () => { const eraAberto = hubAberto; hub = null; hubAberto = false; if (eraAberto) bus.emit('hub', false); agendarHub(); };
        ws.onerror = () => { try { ws.close(); } catch (_) { /* já fechado */ } };
    }
    // sem alarde: o hub só existe no app desktop; fora dele a tentativa é silenciosa e espaçada
    function agendarHub() {
        if (!hubQuer) return;
        setTimeout(conectarHub, espera);
        espera = Math.min(espera * 1.6, 30000);
    }

    function tratarHub(m) {
        if (m.state === 'idle') ignorarHub = false;
        if (m.state != null) O.estado.definir(m.state);
        if (m.intensity != null) bus.emit('audio', m.intensity);
        if (ignorarHub) return;
        if (m.state === 'processing') emitir({ tipo: 'inicio' });
        if (m.user_text) emitir({ tipo: 'usuario', texto: String(m.user_text) });
        if (m.tier) emitir({ tipo: 'modelo', nome: String(m.tier) });
        if (m.ai_chunk) emitir({ tipo: 'texto', texto: String(m.ai_chunk) });
        if (m.provenance) emitir(O.sse.normalizar({ provenance: m.provenance }));
        if (m.tool) emitir(O.sse.normalizar({ tool: m.tool }));
        if (m.approval) emitir(O.sse.normalizar({ approval: m.approval }));
        if (typeof m.error === 'string') emitir({ tipo: 'erro', mensagem: m.error });
        if (m.state === 'idle') emitir({ tipo: 'fim' });
    }

    /* ── API pública ───────────────────────────────────────────────────── */
    const transport = {
        conectarHub,
        hubAberto: () => hubAberto,

        /** @returns {'hub'|'sse'} por onde a mensagem foi — o chat usa para não duplicar o eco do hub */
        enviar({ texto, modelo = 'auto', skills = [] }) {
            const ponte = window.pywebview?.api;
            if (api.usarHub() && ponte?.process_command && hubAberto) {
                // `ignorarHub` NÃO é limpo aqui: sobra de um pedido cancelado não entra na resposta nova
                ponte.process_command(texto, modelo);
                return 'hub';
            }
            lerStream('/chat', { json: api.pedidoChat({ texto, modelo, skills }) }, 'O cérebro recusou o pedido');
            return 'sse';
        },

        /** continua a resposta depois que o usuário aprovou a ação */
        retomar(id) {
            lerStream(`/approvals/${encodeURIComponent(id)}/resume`, api.pedidoRetomada(), 'Não consegui retomar a ação');
            return 'sse';
        },

        cancelar() {
            if (abortar) { abortar(); return; }
            if (hubAberto) {
                ignorarHub = true;                              // descarta o que ainda chegar, até o idle
                window.pywebview?.api?.cancel_command?.();      // app novo: o launcher para o stream e manda o idle na hora
                emitir({ tipo: 'fim', interrompida: true });
            }
        },
        /** parou pelo hub e o launcher ainda não confirmou (idle): novo envio espera */
        cancelando: () => ignorarHub,
    };
    O.transport = transport;
})();
