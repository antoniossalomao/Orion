/* ==========================================================================
   ORION — chat.js | mensagens, streaming, ferramentas e aprovações
   Consome os eventos de `transport` (bus `chat:evento`). Garantias:
     · markdown renderizado no máximo 1× por quadro (rAF), só o trecho ainda instável;
     · o leitor de tela não narra pedaço por pedaço: a resposta pronta é anunciada 1×;
     · rolagem acompanha a resposta só enquanto o usuário não subiu a tela.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, icone, bus, ui, api, anunciar } = O;
    const U = O.util, MD = O.md;

    const MAX_NOS = 240;          // mensagens mantidas no DOM
    const LIMIAR_DEMORA_MS = 8000;
    const PERTO_PX = 28;

    let col, rolagem, vazio, btnFim;
    let atual = null;             // resposta em curso
    let seguir = true;
    let naoLidas = 0;
    let ecoEsperado = null;       // texto que enviamos pelo hub: ignora o eco `user_text`
    let aguardandoRetomada = null; // cartão aprovado cuja execução ainda vai chegar
    let ocupado = false;
    const textoDe = new WeakMap();   // elemento .msg → texto (copiar)

    /* ── utilidades ────────────────────────────────────────────────────── */
    const distFim = () => rolagem.scrollHeight - rolagem.scrollTop - rolagem.clientHeight;
    function irAoFim(suave = false) {
        rolagem.scrollTo({ top: rolagem.scrollHeight, behavior: suave && !O.movimentoReduzido() ? 'smooth' : 'instant' });
    }
    function atualizarBotaoFim() {
        const longe = distFim() > PERTO_PX * 3 && rolagem.scrollHeight > rolagem.clientHeight;
        btnFim.dataset.show = String(longe);
        btnFim.querySelector('.count').textContent = naoLidas > 0 ? String(naoLidas) : '';
        btnFim.tabIndex = longe ? 0 : -1;
    }
    function acompanhar() { if (seguir) { irAoFim(); } atualizarBotaoFim(); }

    function mostrar(m, animar) {
        if (!animar || O.movimentoReduzido()) { m.dataset.visible = 'true'; return; }
        requestAnimationFrame(() => requestAnimationFrame(() => { m.dataset.visible = 'true'; }));
    }
    function atualizarVazio() {
        vazio.hidden = !!col.querySelector('.msg, .day-sep, .msg-system');
    }
    function podar() {
        if (O.historico?.sessao()) return;
        const nos = col.querySelectorAll('.msg, .day-sep, .msg-system');
        for (let i = 0; i < nos.length - MAX_NOS; i++) nos[i].remove();
    }
    let destinoHistorico = null;
    function acrescentar(no) {
        if (destinoHistorico) { destinoHistorico.append(no); return; }
        podar();
        col.append(no);
        atualizarVazio();
    }

    const botaoAcao = (acao, rotulo, icon) =>
        el('button', { class: 'icon-btn', type: 'button', 'aria-label': rotulo, dataset: { acao, tip: rotulo, tipPos: 'bottom' }, disabled: acao === 'ouvir' && !api.suporta('tts'), title: acao === 'ouvir' && !api.suporta('tts') ? 'Resposta por voz indisponível neste backend' : null, html: icone(icon) });

    /* ── usuário ───────────────────────────────────────────────────────── */
    function usuario(texto, { anexos = [], skills = [], animar = true } = {}) {
        const m = el('div', { class: 'msg msg-user' }, el('div', { class: 'bubble', text: texto }));
        if (anexos.length) {
            m.append(el('div', { class: 'attach-note' }, el('span', { html: icone('clip') }), `${anexos.map(a => a).join(', ')}`));
        }
        for (const skill of skills) m.append(el('div', { class: 'attach-note', text: `Skill: ${skill.id} · ${skill.origin} · ${skill.version}` }));
        m.append(el('div', { class: 'msg-actions' }, botaoAcao('copiar', 'Copiar mensagem', 'copy')));
        textoDe.set(m, texto);
        acrescentar(m);
        mostrar(m, animar);
        if (!destinoHistorico) { seguir = true; irAoFim(); atualizarBotaoFim(); }
        return m;
    }

    /* ── resposta do Orion ─────────────────────────────────────────────── */
    function criarOrion({ quando = new Date(), pensando = true, animar = true } = {}) {
        const hora = typeof quando === 'string' ? quando : U.hora(quando);
        const prose = el('div', { class: 'prose selectable', hidden: true });
        const atividade = el('div', { class: 'activity', hidden: true, role: 'group', 'aria-label': 'Ferramentas usadas' });
        const tier = el('span', { class: 'msg-tier' });
        const dur = el('span', { class: 'msg-dur' });
        const pens = pensando
            ? el('div', { class: 'thinking', role: 'status' }, el('span', { class: 'dots', 'aria-hidden': 'true', html: '<i></i><i></i><i></i>' }), 'Pensando…')
            : null;
        const demora = el('div', { class: 'slow-note', text: 'Ainda processando… alguns modelos levam alguns segundos.' });
        const principal = el('div', { class: 'msg-main' },
            el('div', { class: 'msg-meta' }, el('span', { class: 'msg-who', text: 'Orion' }), el('time', { class: 'msg-time', text: hora }), tier, dur),
            atividade, pens, prose, demora);
        const m = el('div', { class: 'msg msg-orion', dataset: { streaming: pensando ? 'true' : 'false' } },
            el('span', { class: 'msg-avatar', 'aria-hidden': 'true', html: '<span class="belt-mark"><i></i><i></i><i></i></span>' }), principal);
        const a = { el: m, principal, prose, atividade, tier, dur, t0: performance.now(), pens, demora, texto: '', rs: MD.criarRenderStreaming(),
                    timer: null, cartoes: new Map(), chips: [], erro: false, acoes: false, fim: false };
        a.desenhar = U.noProximoQuadro(() => desenhar(a));
        acrescentar(m);
        mostrar(m, animar);
        return a;
    }

    function desenhar(a) {
        a.prose.innerHTML = a.rs.renderizar(a.texto);
        if (a === atual) acompanhar();
    }

    function iniciar({ pensando = true } = {}) {
        if (atual && !atual.fim) return atual;
        atual = criarOrion({ pensando });
        if (aguardandoRetomada) { atual.retoma = aguardandoRetomada; aguardandoRetomada = null; }
        if (!seguir) { naoLidas++; }
        atual.timer = setTimeout(() => { atual.demora.dataset.show = 'true'; }, LIMIAR_DEMORA_MS);
        setOcupado(true);
        if (O.estado.atual === 'idle') O.estado.definir('processing');
        if (pensando) { O.som?.envio?.(); anunciar('Orion está pensando.'); }
        acompanhar();
        return atual;
    }

    function tirarPensando(a) {
        clearTimeout(a.timer);
        a.demora.dataset.show = 'false';
        if (a.pens) { a.pens.remove(); a.pens = null; }
    }

    function receberTexto(texto) {
        const a = atual || iniciar({ pensando: false });
        if (!a.texto) {
            tirarPensando(a);
            a.prose.hidden = false;
            a.el.dataset.streaming = 'true';
            if (O.estado.atual !== 'listening' || !O.voz?.ativa()) O.estado.definir('speaking');
        }
        a.texto += texto;
        a.desenhar();
    }

    /* ── ferramentas ───────────────────────────────────────────────────── */
    const ROTULO_ESTADO = { ok: 'permitida', concluida: 'concluída', processando: 'processando', espera: 'aguardando aprovação', negado: 'negada', falha: 'falhou', cancelada: 'interrompida' };
    function estadoFerramenta(ev) {
        const states = { processing: 'processando', waiting_approval: 'espera', completed: 'concluida', failed: 'falha', cancelled: 'cancelada', denied: 'negado' };
        if (states[ev.estado]) return states[ev.estado];
        if (ev.erro || ev.decisao === 'deny') return 'negado';
        if (ev.decisao === 'confirm' && !ev.aprovada) return 'espera';
        return 'ok';
    }
    function chipFerramenta(ev) {
        const a = atual || iniciar({ pensando: false });
        tirarPensando(a);
        const est = estadoFerramenta(ev);
        const icon = ['ok','concluida'].includes(est) ? 'check' : ['negado','falha','cancelada'].includes(est) ? 'close' : 'tool';
        // o evento não traz id da chamada: uma chamada que esperava aprovação é "retomada" no mesmo chip;
        // qualquer outra vira chip novo (duas chamadas da mesma ferramenta não se sobrescrevem)
        let chip = [...a.chips].reverse().find(c => ev.chamada ? c.dataset.chamada === ev.chamada : c.dataset.nome === ev.nome && c.dataset.estado === 'espera');
        if (!chip) {
            chip = el('span', { class: 'tool-chip', role: 'img', dataset: { nome: ev.nome } });
            a.chips.push(chip);
            a.atividade.append(chip);
            a.atividade.hidden = false;
        }
        chip.dataset.estado = est;
        if (ev.chamada) chip.dataset.chamada = ev.chamada;
        chip.title = [ev.origem, ev.revisao ? `revisão ${ev.revisao.slice(0,12)}` : '', ev.resumo || ev.motivo || ev.erro].filter(Boolean).join(' · ');
        chip.setAttribute('aria-label', `Ferramenta ${ev.rotulo || ev.nome}: ${ROTULO_ESTADO[est]}${ev.motivo ? '. ' + ev.motivo : ''}`);
        chip.innerHTML = icone(icon);
        chip.append((ev.rotulo || ev.nome).replace(/_/g, ' '));
        acompanhar();
    }

    /* ── aprovações ────────────────────────────────────────────────────── */
    function textoArgs(args) {
        const frag = document.createDocumentFragment();
        const entradas = Object.entries(args || {});
        if (!entradas.length) { frag.append('(sem argumentos)'); return frag; }
        entradas.forEach(([k, v], i) => {
            const val = typeof v === 'string' ? v : JSON.stringify(v);
            frag.append(el('b', { text: k }), ' ', val, i < entradas.length - 1 ? '\n' : '');  // inteiro: o fim de um comando longo não pode ficar escondido
        });
        return frag;
    }

    function cartaoAprovacao(ev, a = atual || iniciar({ pensando: false }), { depoisDoTexto = false } = {}) {
        tirarPensando(a);
        if (a.cartoes.has(ev.id)) return;
        if (ev.ferramenta === 'criar_evento_agenda') {
            const card = el('div', { class: 'approval', dataset: { id: ev.id } }, el('h4', { text: 'Evento aguardando revisão' }), el('p', { text: 'Confira conta, horários e conteúdo na Agenda antes de confirmar a criação.' }), el('button', { class: 'btn btn-primary btn-sm', type: 'button', text: 'Revisar evento', on: { click: () => { O.app.ir('integracoes'); O.extensions.abrirMcp(); } } }));
            a.cartoes.set(ev.id, card); a.principal.insertBefore(card, a.prose); return;
        }
        const estado = el('span', { class: 'approval-state', role: 'status', 'aria-live': 'polite' });
        // argumento cortado pelo servidor (grande demais): quem decide não vê tudo, então só dá para negar
        const grande = !!ev.truncado;
        const aprovar = grande ? null : el('button', { class: 'btn btn-primary btn-sm', type: 'button', dataset: { decisao: 'aprovar' }, text: 'Aprovar e executar' });
        const negar = el('button', { class: 'btn btn-outline btn-sm', type: 'button', dataset: { decisao: 'negar' }, text: 'Negar' });
        const cartao = el('div', { class: 'approval', role: 'group', 'aria-label': `Aprovação necessária: ${ev.ferramenta}`, dataset: { estado: 'pendente', id: ev.id } },
            el('div', { class: 'approval-head' }, el('span', { html: icone('shield') }), 'Aprovação necessária',
                el('span', { class: 'approval-tool', text: ev.ferramenta })),
            el('p', { class: 'approval-reason', text: ev.motivo || 'Esta ação muda algo no seu computador e precisa do seu aval.' }),
            el('pre', { class: 'approval-args', tabindex: '0', 'aria-label': 'Argumentos da ação' }, textoArgs(ev.args)),
            grande ? el('p', { class: 'approval-warn', role: 'alert', text: 'Os argumentos são grandes demais para revisar aqui, então só dá para negar. Se for legítimo, peça de novo em partes menores.' }) : null,
            el('div', { class: 'approval-actions' }, aprovar, negar, estado));
        a.cartoes.set(ev.id, cartao);
        if (depoisDoTexto) a.prose.after(cartao); else a.principal.insertBefore(cartao, a.prose);
        anunciar(`Aprovação necessária: ${ev.ferramenta}. ${ev.motivo || ''}`);
        O.som?.atencao?.();
        bus.emit('atencao', { tipo: 'aprovacao', ferramenta: ev.ferramenta });
        acompanhar();
    }

    function marcarCartao(cartao, est, texto, travar = true) {
        cartao.dataset.estado = est;
        cartao.querySelector('.approval-state').textContent = texto;
        if (travar) cartao.querySelectorAll('button').forEach(b => { b.disabled = true; });
    }

    /** espera a resposta em andamento acabar: a retomada precisa abrir uma mensagem nova, não misturar texto */
    const esperarOcioso = () => (!ocupado ? Promise.resolve() : new Promise(resolver => {
        const solta = bus.on('chat:ocupado', v => { if (!v) { solta(); resolver(); } });
    }));

    async function decidir(cartao, aprovada) {
        const id = cartao.dataset.id;
        cartao.querySelectorAll('button').forEach(b => { b.disabled = true; });
        cartao.querySelector('.approval-state').textContent = ocupado ? 'Aguardando a resposta terminar…' : 'Enviando…';
        await esperarOcioso();
        cartao.querySelector('.approval-state').textContent = 'Enviando…';
        try {
            await api.decidir(id, aprovada);
        } catch (e) {
            if (e.status === 404 || e.status === 409) { marcarCartao(cartao, 'expirada', e.message); anunciar(e.message); return; }
            cartao.querySelectorAll('button').forEach(b => { b.disabled = false; });
            cartao.querySelector('.approval-state').textContent = '';
            ui.toast(e.message, { tipo: 'erro', acao: e.status === 401 || e.status === 403
                ? { rotulo: 'Configurar', fn: () => O.app.ir('config') } : null });
            return;
        }
        if (aprovada) {
            marcarCartao(cartao, 'aprovada', 'Aprovada · executando…');
            aguardandoRetomada = cartao;
            anunciar('Ação aprovada. Executando.');
            O.transport.retomar(id);
        } else {
            marcarCartao(cartao, 'negada', 'Negada · nada foi executado.');
            anunciar('Ação negada. Nada foi executado.');
        }
    }

    /* ── erro e fim ────────────────────────────────────────────────────── */
    function mostrarErro(ev) {
        const a = atual || iniciar({ pensando: false });
        tirarPensando(a);
        a.erro = true;
        const caixa = el('div', { class: 'msg-error', role: 'alert' }, el('span', { html: icone('alert') }), el('span', { text: ev.mensagem || 'Algo deu errado.' }));
        const auth = ev.status === 401 || ev.status === 403;
        if (auth) caixa.append(el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Configurar token', dataset: { acao: 'config' } }));
        else if (O.composer?.temPedido()) caixa.append(el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Tentar de novo', dataset: { acao: 'repetir' } }));
        a.principal.insertBefore(caixa, a.demora);
        O.som?.erro?.();
        anunciar(`Erro: ${ev.mensagem}`);
        acompanhar();
    }

    function barraAcoes(a, ultima) {
        const barra = el('div', { class: 'msg-actions' }, botaoAcao('copiar', 'Copiar resposta', 'copy'), botaoAcao('ouvir', 'Ouvir resposta', 'speaker'));
        if (api.suporta('artifacts')) barra.append(botaoAcao('salvar-resultado', 'Salvar resultado', 'copy'));
        if (ultima) barra.append(botaoAcao('repetir', 'Gerar de novo', 'retry'));
        a.principal.append(barra);
        a.acoes = true;
    }
    function marcarUltima() {
        $$('.msg[data-last="true"]', col).forEach(m => { delete m.dataset.last; m.querySelector('[data-acao="repetir"]')?.remove(); });
    }

    function finalizar(a, { interrompida = false } = {}) {
        a.fim = true;
        for (const chip of a.chips) if (chip.dataset.estado === 'processando') {
            chip.dataset.estado = interrompida ? 'cancelada' : 'falha';
            chip.setAttribute('aria-label', `Ferramenta ${chip.dataset.nome}: ${interrompida ? 'interrompida' : 'resultado não confirmado'}`);
        }
        tirarPensando(a);
        if (a.texto) a.dur.textContent = U.fmtDur(performance.now() - a.t0);
        if (a.retoma) {
            const falhou = a.erro || a.chips.some(c => ['negado','falha','cancelada'].includes(c.dataset.estado));
            a.retoma.querySelector('.approval-state').textContent = falhou ? 'Aprovada · a execução falhou.' : 'Aprovada · executada.';
        }
        if (a.texto) a.prose.innerHTML = a.rs.renderizar(a.texto);
        a.el.dataset.streaming = 'false';
        textoDe.set(a.el, a.texto);
        const util = a.texto || a.erro || a.cartoes.size || a.chips.length;
        if (!util) a.el.remove();
        else {
            if (interrompida) a.principal.insertBefore(el('div', { class: 'slow-note', dataset: { show: 'true' }, text: 'Resposta interrompida.' }), a.demora);
            if (a.texto) {
                marcarUltima();
                barraAcoes(a, !!O.composer?.temPedido());
                a.el.dataset.last = 'true';
                anunciar(`Orion respondeu: ${U.truncar(MD.paraFala(a.texto), 180)}`);
                O.som?.mensagem?.();
            }
            if (document.documentElement.dataset.view !== 'chat') {
                $('#badge-chat')?.classList.add('show');
                bus.emit('atencao', { tipo: 'mensagem' });
            }
        }
        atualizarVazio();
        if (atual === a) atual = null;
        setOcupado(false);
        if (!O.voz?.ativa()) O.estado.definir('idle');
        acompanhar();
        bus.emit('sessoes:atualizar');
    }

    function setOcupado(v) {
        if (ocupado === v) return;
        ocupado = v;
        bus.emit('chat:ocupado', v);
    }

    /* ── histórico ─────────────────────────────────────────────────────── */
    function mostrarFontes(a, provenance) {
        if (!provenance || typeof provenance !== 'object') return;
        const rows = value => Array.isArray(value) ? value.slice(0, 128) : [];
        const labels = [
            ...rows(provenance.memoria).map(x => x.fonte ? `Memória: ${x.fonte}` : `Memória: ${x.tipo || 'fonte'} ${x.id || ''}`),
            ...rows(provenance.skills).map(x => `Skill: ${x.id} · ${x.origin} · ${x.version}`),
            ...rows(provenance.contexto_externo).map(x => `Fonte externa: ${x.origin || x.connection || ''} · ${x.kind || ''} · ${x.key || x.uri || x.name || x.reference || ''}`),
        ];
        const activity = rows(provenance.atividades);
        if (!activity.length) labels.push(...rows(provenance.ferramentas).map(x => typeof x === 'string' ? x.replace(/_/g, ' ') : x.nome || x.name));
        const states = { completed:'Concluída', failed:'Falhou', denied:'Negada', waiting_approval:'Aguardando aprovação', cancelled:'Interrompida', processing:'Processando' };
        const details = el('details', { class: 'history-sources' }, el('summary', { text: 'Fontes e atividade' }),
            ...[...new Set(labels)].filter(Boolean).map(text => el('p', { text })),
            ...activity.map(item => el('p', { text: `${item.label || item.name} · ${states[item.state] || item.decision || 'Registrada'}${item.origin ? ' · ' + item.origin : ''}${item.revision ? ' · revisão ' + item.revision.slice(0, 12) : ''}${item.summary ? ' · ' + item.summary : ''}` })));
        a.fontes?.remove(); a.fontes = null;
        if (labels.length || activity.length) { a.principal.append(details); a.fontes = details; }
    }

    function renderHistorico(msgs, { antes = false } = {}) {
        const altura = rolagem.scrollHeight, top = rolagem.scrollTop;
        destinoHistorico = document.createDocumentFragment();
        let dia = '';
        for (const m of (msgs || [])) {
            const rotulo = m.timestamp ? U.rotuloDia(m.timestamp) : '';
            if (rotulo && rotulo !== dia) { dia = rotulo; acrescentar(el('div', { class: 'day-sep', text: rotulo })); }
            if (m.role === 'user') {
                const n = usuario(String(m.content ?? ''), { animar: false, skills: m.provenance?.skills || [] });
                if (m.id) { n.dataset.messageId = String(m.id); n.dataset.sessionId = O.historico.sessao(); if (api.suporta('branches')) n.querySelector('.msg-actions').append(botaoAcao('editar-pedido', 'Editar em novo caminho', 'copy')); }
                if (m.timestamp) n.insertBefore(el('time', { class: 'msg-time', datetime: m.timestamp, text: U.hora(m.timestamp) || '' }), n.querySelector('.msg-actions'));
                continue;
            }
            const a = criarOrion({ quando: m.timestamp ? U.hora(m.timestamp) || '' : '', pensando: false, animar: false });
            a.el.dataset.streaming = 'false';
            if (m.id) a.el.dataset.messageId = String(m.id);
            if (O.historico.sessao()) a.el.dataset.sessionId = O.historico.sessao();
            a.texto = String(m.content ?? '');
            a.prose.hidden = false;
            a.prose.innerHTML = a.rs.renderizar(a.texto);
            textoDe.set(a.el, a.texto);
            barraAcoes(a, false);
            if (m.timestamp) a.el.querySelector('time').setAttribute('datetime', m.timestamp);
            mostrarFontes(a, m.provenance);
        }
        if (antes) col.prepend(destinoHistorico); else col.append(destinoHistorico);
        destinoHistorico = null;
        atualizarVazio();
        if (antes) { rolagem.scrollTop = top + rolagem.scrollHeight - altura; atualizarBotaoFim(); return; }
        $('#badge-chat')?.classList.remove('show');
        seguir = true;
        irAoFim();
        atualizarBotaoFim();
    }

    function limpar() {
        if (ocupado) O.transport.cancelar();      // não deixa um stream órfão escrever numa conversa vazia
        $$('.msg, .day-sep, .msg-system', col).forEach(n => n.remove());
        if (atual) { clearTimeout(atual.timer); atual = null; }
        naoLidas = 0;
        setOcupado(false);
        atualizarVazio();
        atualizarBotaoFim();
        bus.emit('chat:limpo');
    }

    function nota(texto, tipo = '') {
        acrescentar(el('div', { class: `msg-system ${tipo}`.trim(), text: texto }));
        acompanhar();
    }

    async function carregarPendentes() {
        try {
            const lista = await api.aprovacoes();
            if (!Array.isArray(lista) || !lista.length) return;
            const a = criarOrion({ pensando: false, animar: false });
            a.el.dataset.streaming = 'false';
            a.prose.hidden = false;
            a.prose.textContent = lista.length === 1 ? 'Há uma ação esperando o seu aval desde antes:' : `Há ${lista.length} ações esperando o seu aval desde antes:`;
            lista.forEach(p => cartaoAprovacao({ id: String(p.id), ferramenta: String(p.tool || p.tool_name || p.ferramenta || 'ação'),
                motivo: p.reason || p.motivo || '', args: p.args && typeof p.args === 'object' ? p.args : {} }, a, { depoisDoTexto: true }));
            a.fim = true;
        for (const chip of a.chips) if (chip.dataset.estado === 'processando') {
            chip.dataset.estado = interrompida ? 'cancelada' : 'falha';
            chip.setAttribute('aria-label', `Ferramenta ${chip.dataset.nome}: ${interrompida ? 'interrompida' : 'resultado não confirmado'}`);
        }
        } catch (_) { /* sem token ou sem orion.app: não há o que mostrar */ }
    }

    /* ── eventos ───────────────────────────────────────────────────────── */
    function aoEvento(ev) {
        switch (ev.tipo) {
            case 'inicio': iniciar(); break;
            case 'usuario':
                if (ecoEsperado && ev.texto.trim() === ecoEsperado.trim()) { ecoEsperado = null; break; }
                usuario(ev.texto);
                break;
            case 'modelo': (atual || iniciar({ pensando: false })).tier.textContent = ev.nome; break;
            case 'texto': receberTexto(ev.texto); break;
            case 'ferramenta': chipFerramenta(ev); break;
            case 'aprovacao': cartaoAprovacao(ev); break;
            case 'fontes': { const a = atual || iniciar({ pensando:false }); mostrarFontes(a, ev.provenance); if (ev.message_id) a.el.dataset.messageId = String(ev.message_id); if (ev.session_id) a.el.dataset.sessionId = ev.session_id; break; }
            case 'erro': mostrarErro(ev); break;
            case 'fim': if (atual) finalizar(atual, ev); else setOcupado(false); break;
            default: break;
        }
    }

    function clique(e) {
        const link = e.target.closest('a.md-link');
        if (link) { e.preventDefault(); api.abrirExterno(link.href); return; }
        const copiarCodigo = e.target.closest('.md-copy');
        if (copiarCodigo) {
            const code = copiarCodigo.closest('.md-code')?.querySelector('pre code');
            if (code) ui.copiar(code.textContent).then(ok => ok ? ui.piscarOk(copiarCodigo, 'Copiado') : ui.toast('Não consegui copiar.', { tipo: 'erro' }));
            return;
        }
        const dec = e.target.closest('[data-decisao]');
        if (dec) { decidir(dec.closest('.approval'), dec.dataset.decisao === 'aprovar'); return; }
        const acao = e.target.closest('[data-acao]');
        if (!acao) return;
        const msg = acao.closest('.msg');
        switch (acao.dataset.acao) {
            case 'salvar-resultado': O.artifacts.salvarResposta(textoDe.get(msg) || '', Number(msg.dataset.messageId) || null, msg.dataset.sessionId || null); break;
            case 'editar-pedido': O.caminhos.editar(msg.dataset.messageId, msg.dataset.sessionId, textoDe.get(msg) || ''); break;
            case 'copiar': ui.copiar(textoDe.get(msg) || '').then(ok => ok ? ui.piscarOk(acao) : ui.toast('Não consegui copiar.', { tipo: 'erro' })); break;
            case 'ouvir': {
                const t = MD.paraFala(textoDe.get(msg) || '');
                if (t) api.ttsFalar(t).then(() => ui.piscarOk(acao)).catch(err => ui.toast(`Não consegui falar: ${err.message}`, { tipo: 'aviso' }));
                break;
            }
            case 'repetir': O.composer.reenviar(); break;
            case 'config': O.app.ir('config'); break;
            default: break;
        }
    }

    function init() {
        col = $('#chat-col');
        rolagem = $('#chat-scroll');
        vazio = $('#chat-empty');
        btnFim = $('#scroll-down');
        rolagem.addEventListener('scroll', () => {
            if (distFim() < PERTO_PX) { seguir = true; naoLidas = 0; }
            atualizarBotaoFim();
        }, { passive: true });
        // intenção explícita de ler o que ficou para trás: solta o acompanhamento na hora
        const soltar = () => { seguir = false; };
        rolagem.addEventListener('wheel', e => { if (e.deltaY < 0) soltar(); }, { passive: true });
        rolagem.addEventListener('touchmove', () => { if (distFim() > PERTO_PX) soltar(); }, { passive: true });
        rolagem.addEventListener('keydown', e => { if (['ArrowUp', 'PageUp', 'Home'].includes(e.key)) soltar(); });
        btnFim.addEventListener('click', () => { seguir = true; naoLidas = 0; irAoFim(true); atualizarBotaoFim(); });
        col.addEventListener('click', clique);
        bus.on('chat:evento', aoEvento);
        ligarCitar();
        new ResizeObserver(() => { if (seguir) irAoFim(); atualizarBotaoFim(); }).observe(col);
        atualizarVazio();
    }

    /** cartões de aprovação ainda sem decisão (em qualquer ponto da conversa) */
    const pendentes = () => $$('.approval[data-estado="pendente"]', col);
    function irParaPendente() {
        const alvo = pendentes()[0];
        if (!alvo) return false;
        const chegar = () => {
            alvo.setAttribute('tabindex', '-1');
            alvo.scrollIntoView({ block: 'center', behavior: O.movimentoReduzido() ? 'auto' : 'smooth' });
            alvo.focus({ preventScroll: true });   // o foco vai ao cartão, não ao botão: aprovar exige uma escolha explícita
        };
        if (document.documentElement.dataset.view === 'chat') chegar();
        else {
            const solta = bus.on('view', () => { solta(); chegar(); });
            O.app.ir('chat', { foco: false });
        }
        return true;
    }
    const ultimaResposta = () => {
        const msgs = $$('.msg-orion', col).filter(m => (textoDe.get(m) || '').trim());
        return msgs.length ? textoDe.get(msgs[msgs.length - 1]) : '';
    };

    /* "Citar": selecionar um trecho de uma mensagem mostra um botão que o leva ao campo, como `> citação` */
    function ligarCitar() {
        const btn = $('#citar-btn');
        const selecao = () => {
            const s = window.getSelection();
            if (!s || s.isCollapsed || !s.toString().trim() || !col.contains(s.anchorNode)) return '';
            return s.anchorNode.parentElement?.closest('.prose, .bubble') ? s.toString() : '';
        };
        const esconder = () => { btn.hidden = true; };
        col.addEventListener('mouseup', () => setTimeout(() => {
            if (!selecao()) { esconder(); return; }
            const r = window.getSelection().getRangeAt(0).getBoundingClientRect();
            btn.style.left = `${Math.max(8, Math.min(window.innerWidth - 90, r.right - 40))}px`;
            btn.style.top = `${Math.max(8, Math.min(window.innerHeight - 44, r.bottom + 8))}px`;
            btn.hidden = false;
        }, 0));
        document.addEventListener('selectionchange', () => { if (!selecao()) esconder(); });
        rolagem.addEventListener('scroll', esconder, { passive: true });
        btn.addEventListener('mousedown', e => e.preventDefault());   // mantém a seleção até o clique
        btn.addEventListener('click', () => { O.composer.citar(selecao()); esconder(); window.getSelection().removeAllRanges(); });
        O.chat.citarSelecao = () => { const t = selecao(); if (!t) return false; O.composer.citar(t); esconder(); window.getSelection().removeAllRanges(); return true; };
    }

    O.chat = {
        pendentes, irParaPendente, ultimaResposta,
        init, usuario, limpar, renderHistorico, nota, carregarPendentes, irAoFim,
        esperarEco(texto) { ecoEsperado = texto; },
        ocupado: () => ocupado,
        parar: () => O.transport.cancelar(),
        temMensagens: () => !!col.querySelector('.msg'),
    };
})();
