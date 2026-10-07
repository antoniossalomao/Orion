/* ==========================================================================
   ORION — api.js | cliente HTTP do cérebro (legado :8000 e orion.app)
   Detecta o endereço sozinho; toda chamada tem timeout; falha vira ApiError legível.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { prefs } = O;

    class ApiError extends Error {
        constructor(mensagem, { status = 0, rede = false } = {}) {
            super(mensagem);
            this.name = 'ApiError';
            this.status = status;
            this.rede = rede;
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
    const comAuth = caminho => /^\/(approvals|chat|calendar|accounts|branches|documents|activity|notifications|facts|artifacts|projects|skills|plugins|mcp|sessoes|historico|exportar|capabilities\/details)(\/|$|\?)/.test(caminho);

    function cabecalhos(caminho, extra = {}) {
        const h = { ...extra };
        const t = token();
        if (t && comAuth(caminho)) h.Authorization = `Bearer ${t}`;
        return h;
    }

    function mensagemHttp(status, corpo) {
        let detalhe = '';
        try { const j = JSON.parse(corpo); detalhe = j.detail || j.erro || j.error || ''; } catch (_) { detalhe = corpo.slice(0, 160); }
        if (typeof detalhe !== 'string') detalhe = JSON.stringify(detalhe);
        if (status === 401 || status === 403) return 'Acesso negado: confira o token em Configurações › Conexão.';
        if (status === 404) return detalhe || 'Recurso não encontrado neste cérebro.';
        if (status === 409) return detalhe || 'Essa ação já foi tratada.';
        if (status >= 500) return `O cérebro falhou (${status})${detalhe ? ': ' + detalhe : ''}.`;
        return detalhe || `Erro ${status}.`;
    }

    async function req(caminho, { metodo = 'GET', json, form, timeout = 8000, sinal, bruto = false, body, contentType } = {}) {
        const ctrl = new AbortController();
        const tid = setTimeout(() => ctrl.abort(), timeout);
        if (sinal) sinal.addEventListener('abort', () => ctrl.abort(), { once: true });
        const init = { method: metodo, signal: ctrl.signal, headers: cabecalhos(caminho) };
        if (json !== undefined) { init.body = JSON.stringify(json); init.headers['Content-Type'] = 'application/json'; }
        if (form) init.body = form;
        if (body !== undefined) { init.body = body; if (contentType) init.headers['Content-Type'] = contentType; }
        let r;
        try { r = await fetch(base() + caminho, init); }
        catch (e) {
            throw new ApiError(ctrl.signal.aborted && !sinal?.aborted ? 'O cérebro demorou demais para responder.'
                : 'Sem conexão com o cérebro.', { rede: true });
        } finally { clearTimeout(tid); }
        if (!r.ok) throw new ApiError(mensagemHttp(r.status, await r.text().catch(() => '')), { status: r.status });
        if (bruto) return r;
        const tipo = r.headers.get('content-type') || '';
        return tipo.includes('json') ? r.json() : r.text();
    }

    const q = o => Object.entries(o).filter(([, v]) => v != null).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&');

    const rotas = {
        memory_facts: ['/facts'], artifacts: ['/artifacts'], projects: ['/projects'], sessions: ['/sessoes'], history: ['/historico'], history_clear: [], export: ['/exportar'],
        memory_graph: ['/grafo/completo'], memory_categories: ['/memoria/categorias'],
        metrics: ['/metrics'], stats: ['/stats'], stats_history: ['/stats/historico'],
        integrations: ['/integracoes'], upload: ['/upload'], tts: ['/tts/mudo', '/tts/falar'],
        calendar: ['/calendar'], accounts: ['/accounts'], branches: ['/branches'], documents: ['/documents'], activity: ['/activity'], approvals: ['/approvals'], notifications: ['/notifications'], chat: ['/chat'],
        skills: ['/skills'], plugins: ['/plugins'], mcp: ['/mcp/connections'],
    };
    let estado = { backend: 'unknown', api: 'offline', model: 'unknown', features: {}, unavailable: {} };
    let origem = '', emDeteccao = null, geracao = 0;
    class UnsupportedError extends ApiError {
        constructor(recurso) {
            super('Este recurso ainda não está disponível neste backend.');
            this.name = 'UnsupportedError'; this.recurso = recurso; this.indisponivel = true;
        }
    }
    const suporta = recurso => estado.features[recurso] === true;
    function publicar(novo) {
        const mudou = JSON.stringify(estado) !== JSON.stringify(novo);
        estado = novo;
        if (mudou) O.bus.emit('capabilities', estado);
        return estado;
    }
    async function detectar(timeout = 2000, atualizar = false) {
        const endereco = base();
        if (origem !== endereco) { origem = endereco; emDeteccao = null; geracao++; estado = { backend: 'unknown', api: 'offline', model: 'unknown', features: {}, unavailable: {} }; }
        if (emDeteccao) return emDeteccao;
        if (!atualizar && estado.api === 'online') return estado;
        const atual = geracao;
        const tarefa = (async () => {
            let novo;
            try {
                // /health existe nos dois backends: evita sondar rotas ausentes no legado.
                const h = await req('/health', { timeout });
                if (h && h.components && typeof h.components.gateway === 'boolean') {
                    const c = await req('/capabilities', { timeout });
                    if (c.contract_version !== 1 || c.backend !== 'orion' || !c.features || !['ready', 'unavailable'].includes(c.model)) {
                        novo = { backend: 'unknown', api: 'online', model: 'unknown', features: {}, unavailable: {}, incompatible: true };
                    } else novo = c;
                } else if (h?.cerebro && typeof h.cerebro.ok === 'boolean') {
                    const features = Object.fromEntries([...Object.keys(rotas), 'history_clear', 'voice', 'model_selection'].map(k => [k, true]));
                    // Rotas opcionais ausentes são lembradas até trocar de backend.
                    if (estado.backend === 'legacy') Object.assign(features, estado.features);
                    features.chat = h.cerebro.ok;
                    features.skills = false;
                    features.plugins = false; features.mcp = false; features.projects = false; features.artifacts = false; features.memory_facts = false;
                    features.notifications = false; // pertence ao backend novo
                    novo = { backend: 'legacy', api: 'online', model: h.cerebro.ok ? 'ready' : 'unavailable', features, unavailable: estado.backend === 'legacy' ? estado.unavailable : {} };
                } else novo = { backend: 'unknown', api: 'online', model: 'unknown', features: {}, unavailable: {}, incompatible: true };
            } catch (e) {
                novo = { ...estado, api: e.rede ? 'offline' : 'online', model: 'unknown', error: e.message };
                // Uma API nova sem contrato nunca é confundida com o legado.
                if (estado.backend === 'unknown') novo.features = {};
            }
            if (atual !== geracao || endereco !== base()) return detectar(timeout, atualizar);
            return publicar(novo);
        })();
        emDeteccao = tarefa;
        try { return await tarefa; } finally { if (emDeteccao === tarefa) emDeteccao = null; }
    }
    async function recurso(nome, caminho, opcoes) {
        await detectar();
        if (!suporta(nome)) throw new UnsupportedError(nome);
        try { return await req(caminho, opcoes); }
        catch (e) {
            if (e.status === 404 && !/[?&]sessao=/.test(caminho) && (rotas[nome] || []).includes(caminho.split('?')[0])) {
                publicar({ ...estado, features: { ...estado.features, [nome]: false }, unavailable: { ...estado.unavailable, [nome]: 'not_implemented' } });
                throw new UnsupportedError(nome);
            }
            throw e;
        }
    }
    const api = {
        ApiError, UnsupportedError, base, wsBase, token, configurarToken, req,
        detectar, suporta, estado: () => estado,
        perfisPlugins: () => recurso('plugin_profiles', '/plugins/available'),
        instalarPerfil: id => recurso('plugin_profiles', `/plugins/builtin/${encodeURIComponent(id)}`, { metodo: 'POST', timeout: 30000 }),
        plugins: () => recurso('plugins', '/plugins'),
        importarPlugin: arquivo => recurso('plugins', '/plugins/import?update=true', { metodo: 'POST', body: arquivo, contentType: 'application/zip', timeout: 30000 }),
        instalarPlugin: folder => recurso('plugins', '/plugins/install', { metodo: 'POST', json: { folder, update: true }, timeout: 30000 }),
        plugin: (id, action, json) => recurso('plugins', `/plugins/${encodeURIComponent(id)}${action ? '/' + action : ''}`, { metodo: action === '' ? 'DELETE' : 'POST', json, timeout: 30000 }),
        versoesPlugin: id => recurso('plugins', `/plugins/${encodeURIComponent(id)}/versions`),
        configurarMcp: (config, edit = false) => recurso('mcp', `/mcp/connections${edit ? '/' + encodeURIComponent(config.id) : ''}`, { metodo: edit ? 'PUT' : 'POST', json: config }),
        testarMcp: id => recurso('mcp', `/mcp/connections/${encodeURIComponent(id)}/test`, { metodo: 'POST', timeout: 30000 }),
        desativarMcp: id => recurso('mcp', `/mcp/connections/${encodeURIComponent(id)}/disable`, { metodo: 'POST', timeout: 30000 }),
        removerMcp: id => recurso('mcp', `/mcp/connections/${encodeURIComponent(id)}`, { metodo: 'DELETE', timeout: 30000 }),
        conexoesMcp: () => recurso('mcp', '/mcp/connections'),
        pedidoChat: ({ texto, modelo, skills = [] }) => estado.backend === 'orion'
            ? { texto, canal: 'web', ...(skills.length ? { skills } : {}) } : { texto, modelo },
        pedidoRetomada: () => estado.backend === 'orion' ? { json: { canal: 'web' } } : {},
        usarHub: () => estado.backend === 'legacy',
        async ping(timeout = 2000) {
            const t0 = performance.now();
            const c = await detectar(timeout, true);
            return { ok: c.api === 'online', model: c.model, incompatible: c.incompatible,
                ms: c.api === 'online' ? Math.round(performance.now() - t0) : null };
        },
        health: () => req('/health', { timeout: 3000 }),
        metrics: () => recurso('metrics', '/metrics', { timeout: 2500 }),
        stats: () => recurso('stats', '/stats', { timeout: 3000 }),
        statsHistorico: (limite = 48) => recurso('stats_history', `/stats/historico?${q({ limite })}`, { timeout: 3000 }),
        integracoes: () => recurso('integrations', '/integracoes', { timeout: 3500 }),
        categorias: () => recurso('memory_categories', '/memoria/categorias', { timeout: 4000 }),
        grafo: (limite = 500) => recurso('memory_graph', `/grafo/completo?${q({ limite })}`, { timeout: 7000 }),
        fatos: (project_id, query = '') => recurso('memory_facts', `/facts?${q({ project_id, query })}`),
        aprovacoesFatos: project_id => recurso('memory_facts', `/facts/pending?${q({ project_id })}`),
        revisarFato: (id, project_id, json) => recurso('memory_facts', `/facts/${encodeURIComponent(id)}/review?${q({ project_id })}`, { metodo: 'POST', json }),
        executarFato: (id, project_id, approval_id) => recurso('memory_facts', `/facts/${encodeURIComponent(id)}/resume?${q({ project_id })}`, { metodo: 'POST', json: { approval_id } }),
        resultados: (project_id, query = '') => recurso('artifacts', `/artifacts?${q({ project_id, query })}`),
        resultado: (id, project_id, version) => recurso('artifacts', `/artifacts/${encodeURIComponent(id)}?${q({ project_id, version })}`),
        versoesResultado: (id, project_id) => recurso('artifacts', `/artifacts/${encodeURIComponent(id)}/versions?${q({ project_id })}`),
        salvarResultado: (json, project_id, id = null) => recurso('artifacts', `/artifacts${id ? '/' + encodeURIComponent(id) + '/versions' : ''}?${q({ project_id })}`, { metodo: 'POST', json, timeout: 20000 }),
        baixarResultado: (id, project_id, version) => recurso('artifacts', `/artifacts/${encodeURIComponent(id)}/download?${q({ project_id, version })}`, { bruto: true, timeout: 20000 }),
        projetos: (archived = false) => recurso('projects', `/projects?${q({ archived })}`),
        projeto: id => recurso('projects', `/projects/${encodeURIComponent(id)}`),
        criarProjeto: json => recurso('projects', '/projects', { metodo: 'POST', json }),
        editarProjeto: (id, json) => recurso('projects', `/projects/${encodeURIComponent(id)}`, { metodo: 'PATCH', json }),
        usarProjeto: project_id => recurso('projects', '/projects/activate', { metodo: 'POST', json: { project_id } }),
        moverConversa: (id, project_id) => recurso('projects', `/projects/sessions/${encodeURIComponent(id)}`, { metodo: 'PUT', json: { project_id } }),
        skills: () => recurso('skills', '/skills', { timeout: 5000 }),
        sessoes: () => recurso('sessions', '/sessoes', { timeout: 4000 }),
        buscarSessoes: (texto, offset = 0) => recurso('session_search', `/sessoes/busca?${q({ texto, offset })}`, { timeout: 6000 }),
        editarSessao: (id, json) => recurso('session_management', `/sessoes/${encodeURIComponent(id)}`, { metodo: 'PATCH', json, timeout: 5000 }),
        novaSessao: (project_id = null) => recurso('sessions', '/sessoes', { metodo: 'POST', ...(api.suporta('projects') ? { json: { project_id } } : {}), timeout: 5000 }),
        ativarSessao: id => recurso('sessions', '/sessoes/ativar', { metodo: 'POST', json: { sessao_id: id }, timeout: 6000 }),
        historico: (sessao, opcoes = {}) => recurso('history', `/historico?${q({ sessao, ...opcoes })}`, { timeout: 6000 }),
        limparHistorico: sessao => recurso('history_clear', `/historico?${q({ sessao })}`, { metodo: 'DELETE', timeout: 5000 }),
        exportar: (sessao, completo) => recurso('export', `/exportar?${q({ sessao, completo })}`, { timeout: 8000 }),
        ttsMudo: mudo => recurso('tts', '/tts/mudo', { metodo: 'POST', json: { mudo }, timeout: 3000 }),
        ttsFalar: texto => recurso('tts', '/tts/falar', { metodo: 'POST', json: { texto }, timeout: 4000 }),
        upload(arquivo) {
            const f = new FormData();
            f.append('file', arquivo);
            return recurso('upload', '/upload', { metodo: 'POST', form: f, timeout: 120000 });
        },
        propostasEventos: project_id => recurso('calendar', `/calendar/proposals?${q({ project_id })}`),
        proporEvento: (project_id, json) => recurso('calendar', `/calendar/proposals?${q({ project_id })}`, { metodo: 'POST', json }),
        revisarEvento: (id, project_id, reviewed_digest) => recurso('calendar', `/calendar/proposals/${id}/review?${q({ project_id })}`, { metodo: 'POST', json: { reviewed_digest } }),
        criarEvento: (id, project_id, approval_id) => recurso('calendar', `/calendar/proposals/${id}/resume?${q({ project_id })}`, { metodo: 'POST', json: { approval_id }, timeout: 30000 }),
        agendas: () => recurso('calendar', '/calendar'),
        vincularAgenda: json => recurso('calendar', '/calendar/bind', { metodo: 'POST', json }),
        contas: () => recurso('accounts', '/accounts'),
        criarConta: json => recurso('accounts', '/accounts', { metodo: 'POST', json }),
        autorizarConta: id => recurso('accounts', `/accounts/${id}/authorize`, { metodo: 'POST' }),
        revogarConta: id => recurso('accounts', `/accounts/${id}/revoke`, { metodo: 'POST' }),
        caminhos: id => recurso('branches', `/branches/${encodeURIComponent(id)}`),
        ramificarPedido: (id, session_id, text) => recurso('branches', `/branches/messages/${id}`, { metodo: 'POST', json: { session_id, text } }),
        documentos: project_id => recurso('documents', `/documents?${q({ project_id })}`),
        ingerirDocumento: (file, project_id) => recurso('documents', `/documents?${q({ name: file.name, project_id })}`, { metodo: 'POST', body: file, contentType: 'application/octet-stream', timeout: 60000 }),
        repetirDocumento: (id, project_id) => recurso('documents', `/documents/${id}/retry?${q({ project_id })}`, { metodo: 'POST', timeout: 60000 }),
        baixarDocumento: (id, project_id) => recurso('documents', `/documents/${id}/download?${q({ project_id })}`, { bruto: true }),
        atividade: (project_id, unread = false) => recurso('activity', `/activity?${q({ project_id, unread })}`),
        preferenciasAtividade: (project_id, json) => recurso('activity', `/activity/preferences?${q({ project_id })}`, { metodo: 'PUT', json }),
        lerAviso: (id, project_id) => recurso('notifications', `/notifications/${id}/ack?${q({ project_id })}`, { metodo: 'POST' }),
        aprovacoes: () => recurso('approvals', '/approvals', { timeout: 4000 }),
        decidir: (id, aprovada) => recurso('approvals', `/approvals/${encodeURIComponent(id)}/decide`,
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
    prefs.assinar('base_url', () => { sincronizarImagens(); origem = ''; geracao++; emDeteccao = null; publicar({ backend: 'unknown', api: 'offline', model: 'unknown', features: {}, unavailable: {} }); detectar(); });
})();
