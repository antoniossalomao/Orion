/* Avisos duráveis: leitura explícita, sem popups de reconexão. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus } = O;
    const scope = () => O.projects.current()?.id || null;
    let active = false, generation = 0, filter, status, list, decisions, preferences;
    const button = (text, fn) => el('button', { class: 'btn btn-outline btn-sm', type: 'button', text, on: { click: fn } });
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
                n.delivered_at ? null : button('Marcar como lido', async () => { try { await api.lerAviso(n.id, project); await load(); } catch (error) { status.textContent = error.message; } }))));
            decisions.replaceChildren(el('h3', { text: 'Aprovações pendentes' }), ...data.approvals.map(a => el('article', { class: 'card fact-item' },
                el('p', { text: `${a.title} · ${a.tool}` }), el('p', { class: 'extension-hint', text: a.reason }),
                button('Revisar decisão', async () => { const session = O.sidebar.sessoes().find(s => s.sessao_id === a.session_id); if (session) await O.sidebar.abrir(session); else await O.historico.abrir(a.session_id); O.app.ir(a.target); }))),
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
