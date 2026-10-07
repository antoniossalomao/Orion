/* ==========================================================================
   ORION — api.js | cliente HTTP do cérebro (legado :8000 e orion.app)
   Detecta o endereço sozinho; toda chamada tem timeout; falha vira ApiError legível.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { prefs } = O;

    class ApiError extends Error {
        constructor(mensagem, { status = 0, rede = false, esperar = 0 } = {}) {
            super(mensagem);
            this.name = 'ApiError';
            this.status = status;
            this.rede = rede;
            this.esperar = esperar;   // segundos até poder tentar de novo (429 do login)
        }
    }

    let tokenDesktop = '';

    /** Endereço do cérebro: o configurado > a origem desta página (http/https) > 127.0.0.1:8000 */
    function base() {
        const b = String(prefs.get('base_url') || '').trim().replace(/\/+$/, '');
        if (b) return b;
        if (location.protocol === 'http:' || location.protocol === 'https:') return location.origin;
        return 'http://127.0.0.1:8000';   // 127.0.0.1, nunca localhost (IPv6 custa ~2 s no Windows)
    }
    const wsBase = () => base().replace(/^http/, 'ws');
    const token = () => String(prefs.get('token') || '').trim() || tokenDesktop;
    const configurarToken = t => { tokenDesktop = String(t || ''); };

    /** rotas que exigem o token do orion.app; o legado não conhece cabeçalho Authorization */
    const comAuth = caminho => /^\/(approvals|chat|painel|sessoes|historico)(\/|$|\?)/.test(caminho);

    function cabecalhos(caminho, extra = {}) {
        const h = { ...extra };
        const t = token();
        if (t && comAuth(caminho)) h.Authorization = `Bearer ${t}`;
        return h;
    }

    function mensagemHttp(status, corpo, caminho = '') {
        let detalhe = '';
        try { const j = JSON.parse(corpo); detalhe = j.detail || j.erro || j.error || ''; } catch (_) { detalhe = corpo.slice(0, 160); }
        if (typeof detalhe !== 'string') detalhe = JSON.stringify(detalhe);
        if (caminho === '/auth/password' && detalhe) return detalhe;   // o servidor já explica (senha atual errada, senha fraca)
        if (caminho === '/auth/login') {
            if (status === 401) return 'Usuário ou senha incorretos.';
            if (status === 429) return 'Muitas tentativas erradas. Espere um pouco para tentar de novo.';
            if (status === 503) return 'Ainda não há senha definida: rode "orion set-password" no computador do Orion.';
        }
        if (status === 401 || status === 403) return 'Acesso negado: entre com a senha ou confira o token em Configurações › Conexão.';
        if (status === 404) return detalhe || 'Recurso não encontrado neste cérebro.';
        if (status === 409) return detalhe || 'Essa ação já foi tratada.';
        if (status >= 500) return `O cérebro falhou (${status})${detalhe ? ': ' + detalhe : ''}.`;
        return detalhe || `Erro ${status}.`;
    }

    async function req(caminho, { metodo = 'GET', json, form, timeout = 8000, sinal, bruto = false, semAviso = false } = {}) {
        const ctrl = new AbortController();
        const tid = setTimeout(() => ctrl.abort(), timeout);
        if (sinal) sinal.addEventListener('abort', () => ctrl.abort(), { once: true });
        const init = { method: metodo, signal: ctrl.signal, headers: cabecalhos(caminho) };
        if (json !== undefined) { init.body = JSON.stringify(json); init.headers['Content-Type'] = 'application/json'; }
        if (form) init.body = form;
        let r;
        try { r = await fetch(base() + caminho, init); }
        catch (e) {
            throw new ApiError(ctrl.signal.aborted && !sinal?.aborted ? 'O cérebro demorou demais para responder.'
                : 'Sem conexão com o cérebro.', { rede: true });
        } finally { clearTimeout(tid); }
        if (!r.ok) {
            // sessão vencida ou ainda sem login: o app abre a tela de entrada (ver app.js)
            if (r.status === 401 && comAuth(caminho) && !token() && !semAviso) O.bus.emit('auth:necessario');
            throw new ApiError(mensagemHttp(r.status, await r.text().catch(() => ''), caminho),
                { status: r.status, esperar: Number(r.headers.get('retry-after')) || 0 });
        }
        if (bruto) return r;
        const tipo = r.headers.get('content-type') || '';
        return tipo.includes('json') ? r.json() : r.text();
    }

    const q = o => Object.entries(o).filter(([, v]) => v != null).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&');

    const api = {
        ApiError, base, wsBase, token, configurarToken, req,

        /** {ok, ms}: tenta o ping do legado (`/`) e cai para `/health` (orion.app) */
        async ping(timeout = 2000) {
            const t0 = performance.now();
            for (const caminho of ['/', '/health']) {
                try { await req(caminho, { timeout }); return { ok: true, ms: Math.round(performance.now() - t0) }; }
                catch (e) { if (!e.rede && e.status !== 404) return { ok: true, ms: Math.round(performance.now() - t0) }; }
            }
            return { ok: false, ms: null };
        },
        health: () => req('/health', { timeout: 3000 }),
        metrics: () => req('/metrics', { timeout: 2500 }),
        stats: () => req('/stats', { timeout: 3000 }),
        statsHistorico: (limite = 48) => req(`/stats/historico?${q({ limite })}`, { timeout: 3000 }),
        integracoes: () => req('/integracoes', { timeout: 3500 }),
        /** estado do Orion numa resposta só (orion.app: modelos, CLIs, aprovações, política, jobs) */
        painel: () => req('/painel', { timeout: 5000 }),
        categorias: () => req('/memoria/categorias', { timeout: 4000 }),
        grafo: (limite = 500) => req(`/grafo/completo?${q({ limite })}`, { timeout: 7000 }),
        sessoes: () => req('/sessoes', { timeout: 4000 }),
        novaSessao: () => req('/sessoes', { metodo: 'POST', timeout: 5000 }),
        ativarSessao: id => req('/sessoes/ativar', { metodo: 'POST', json: { sessao_id: id }, timeout: 6000 }),
        renomearSessao: (id, titulo) => req(`/sessoes/${encodeURIComponent(id)}`, { metodo: 'PATCH', json: { titulo }, timeout: 5000 }),
        fixarSessao: (id, favorita) => req(`/sessoes/${encodeURIComponent(id)}`, { metodo: 'PATCH', json: { favorita }, timeout: 5000 }),
        apagarSessao: id => req(`/sessoes/${encodeURIComponent(id)}`, { metodo: 'DELETE', timeout: 5000 }),
        historico: sessao => req(`/historico?${q({ sessao })}`, { timeout: 6000 }),
        limparHistorico: () => req('/historico', { metodo: 'DELETE', timeout: 5000 }),
        exportar: () => req('/exportar', { timeout: 8000 }),
        ttsMudo: mudo => req('/tts/mudo', { metodo: 'POST', json: { mudo }, timeout: 3000 }),
        ttsFalar: texto => req('/tts/falar', { metodo: 'POST', json: { texto }, timeout: 4000 }),
        upload(arquivo) {
            const f = new FormData();
            f.append('file', arquivo);
            return req('/upload', { metodo: 'POST', form: f, timeout: 120000 });
        },
        /** {configured, authenticated, token_auth}; null no legado (sem login) ou sem conexão */
        async authStatus() {
            try { return await req('/auth/status', { timeout: 3000 }); } catch (_) { return null; }
        },
        login: (usuario, senha) => req('/auth/login', { metodo: 'POST', json: { usuario, senha }, timeout: 20000 }),
        logout: () => req('/auth/logout', { metodo: 'POST', timeout: 5000 }),
        trocarSenha: (atual, nova) => req('/auth/password', { metodo: 'POST', json: { senha_atual: atual, nova }, timeout: 20000 }),
        /** `semAviso`: consulta de fundo; um 401 não reabre a tela de entrada */
        aprovacoes: ({ semAviso = false } = {}) => req('/approvals', { timeout: 4000, semAviso }),
        decidir: (id, aprovada) => req(`/approvals/${encodeURIComponent(id)}/decide`,
                                       { metodo: 'POST', json: { approved: !!aprovada }, timeout: 6000 }),
        /** abre o link fora do app: no desktop pela ponte do pywebview, na web numa aba nova */
        abrirExterno(url) {
            if (!/^(https?:\/\/|mailto:)/i.test(url)) return false;
            const ponte = window.pywebview?.api;
            if (ponte?.open_external) { ponte.open_external(url); return true; }
            window.open(url, '_blank', 'noopener,noreferrer');
            return true;
        },
    };

    O.api = api;
    // as imagens que o markdown aceita dependem do endereço do cérebro
    const sincronizarImagens = () => O.md.configurar({ basesImagens: [`${base()}/imagens/`] });
    sincronizarImagens();
    prefs.assinar('base_url', sincronizarImagens);
})();
