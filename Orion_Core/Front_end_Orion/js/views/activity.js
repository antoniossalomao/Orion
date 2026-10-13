/* Avisos duráveis: leitura explícita, sem popups de reconexão. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus, ui } = O;
    const scope = () => O.projects.current()?.id || null;
    let active = false, generation = 0, filter, status, list, decisions, preferences;
    const button = (text, fn) => el('button', { class: 'btn btn-outline btn-sm', type: 'button', text, on: { click: fn } });
    /** avisos do ciclo de sono com decisão do Antônio (regra 54): fatos repetidos e fatos que se contradizem */
    function decisoesDoSono(n, project, reload) {
        const dup = /^sono:dup:(\d+):(\d+)$/.exec(n.ref || ''), conf = /^sono:conflito:(\d+):(\d+)$/.exec(n.ref || '');
        if ((!dup && !conf) || n.delivered_at) return null;
        const fim = async fn => { try { await fn(); await api.lerAviso(n.id, project); await reload(); } catch (error) { status.textContent = error.message; } };
        if (conf) {
            const corpo = { older_id: +conf[1], newer_id: +conf[2] };
            return el('div', { class: 'extension-actions', dataset: { sono: 'conflito' } },
                button('O novo vale', () => fim(() => api.resolverConflito(project, { ...corpo, decision: 'novo' }))),
                button('Os dois valem', () => fim(() => api.resolverConflito(project, { ...corpo, decision: 'ambos' }))));
        }
        const apagar = (id, rotulo) => button(rotulo, async () => {
            const ok = await ui.confirmar({ titulo: 'Apagar o fato repetido?', ok: 'Apagar', perigo: true, texto: `O fato ${id} sai da memória, do índice de busca e dos vetores. O outro fica.` });
            if (ok) fim(() => api.apagarFato(id, project));
        });
        return el('div', { class: 'extension-actions', dataset: { sono: 'repetido' } }, apagar(+dup[1], `Apagar o repetido (#${dup[1]})`), apagar(+dup[2], `Apagar o repetido (#${dup[2]})`));
    }
    /** lembrete (inclusive o insistente): "Feito" conclui e para de insistir; "Adiar 1 h" volta daqui a uma hora (E9.3) */
    function acoesDoLembrete(n, project, reload) {
        const achou = /^reminder:(\d+)(?::n\d+)?$/.exec(n.ref || '');
        if (!achou) return null;
        const agir = (acao, rotulo) => button(rotulo, async () => {
            try { await api.agirNoLembrete(+achou[1], acao, project); await api.lerAviso(n.id, project).catch(() => {}); await reload(); }
            catch (error) { status.textContent = error.message; }
        });
        return el('div', { class: 'extension-actions', dataset: { lembrete: achou[1] } }, agir('feito', 'Feito'), agir('adiar', 'Adiar 1 h'));
    }
    async function load() {
        const token = ++generation, project = scope(), source = api.base();
        if (!api.suporta('activity')) { status.textContent = 'Atividade ainda indisponível neste backend.'; return; }
        try {
            const data = await api.atividade(project, filter.value === 'unread');
            if (token !== generation || project !== scope() || source !== api.base()) return;
            status.textContent = `${project ? O.projects.current().name : 'Pessoal'} · ${data.notifications.filter(n => !n.delivered_at).length} aviso(s) não lido(s)`;
            // Preferences affect optional notices. Decisions remain accessible.
            list.replaceChildren(...data.notifications.filter(n => filter.value !== 'pending' && (data.preferences[n.kind] !== false || filter.value === 'all')).map(n => el('article', { class: 'card fact-item', dataset: { notificationId: String(n.id) } },
                el('h3', { text: n.kind }), el('p', { class: 'fact-text', text: n.text }),
                el('p', { class: 'extension-hint', text: `${new Date(n.created_at * 1000).toLocaleString('pt-BR')} · ${n.delivered_at ? 'Lido' : 'Não lido'}` }),
                decisoesDoSono(n, project, load), acoesDoLembrete(n, project, load),
                n.delivered_at ? null : button('Marcar como lido', async () => { try { await api.lerAviso(n.id, project); await load(); } catch (error) { status.textContent = error.message; } }))));
            decisions.replaceChildren(el('h3', { text: 'Aprovações pendentes' }), ...data.approvals.map(a => el('article', { class: 'card fact-item' },
                el('p', { text: `${a.title} · ${a.tool}` }), el('p', { class: 'extension-hint', text: a.reason }),
                button('Revisar decisão', async () => { const session = O.sidebar.sessoes().find(s => s.sessao_id === a.session_id); if (session) await O.sidebar.abrir(session); else await O.historico.abrir(a.session_id); O.app.ir(a.target); if (a.tool === 'criar_evento_agenda') O.extensions.abrirMcp(); }))),
                data.approvals.length ? null : el('p', { class: 'extension-hint', text: 'Nenhuma decisão pendente neste contexto.' }));
            preferences.replaceChildren(...[['completion', 'Conclusões'], ['question', 'Perguntas'], ['approval', 'Destaque de aprovações']].map(([key, label]) => {
                const input = el('input', { type: 'checkbox' }); input.checked = data.preferences[key];
                input.addEventListener('change', async () => { try { await api.preferenciasAtividade(project, { ...data.preferences, [key]: input.checked }); await load(); } catch (error) { status.textContent = error.message; } });
                return el('label', { class: 'extension-check' }, input, el('span', { text: label }));
            }));
        } catch (error) { if (token === generation) status.textContent = error.message; }
    }
    O.views.atividade = {
        init() {
            filter = el('select', { class: 'input', 'aria-label': 'Filtrar atividade' }, ...[['all', 'Todos'], ['unread', 'Não lidos'], ['pending', 'Aprovações']].map(([value, text]) => el('option', { value, text })));
            status = el('p', { role: 'status' }); list = el('div'); decisions = el('section', { 'aria-label': 'Decisões revisáveis' }); preferences = el('div', { class: 'extension-actions' });
            $('#activity-root').append(el('div', { class: 'page-head' }, el('div', {}, el('h2', { text: 'Sua atividade' }), el('p', { text: 'Avisos e decisões, no seu tempo.' })), button('Atualizar', load)), filter, preferences, status, decisions, list);
            filter.addEventListener('change', load);
            for (const event of ['sessoes', 'capabilities']) bus.on(event, () => { generation++; list.replaceChildren(); decisions.replaceChildren(); preferences.replaceChildren(); if (active) load(); });
        }, ativar() { active = true; load(); }, desativar() { active = false; generation++; }
    };
})();
