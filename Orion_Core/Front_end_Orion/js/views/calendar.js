/* Agenda: conta e fuso apresentados explicitamente antes de vincular. */
(function () {
    'use strict';
    const O = window.Orion, { el, api } = O, X = O.extensions;
    const field = (name, input) => el('label', { class: 'extension-field' }, el('span', { text: name }), input);
    async function bind() {
        const source = api.base(), connections = await api.conexoesMcp(), context = O.projects.options('Contexto da Agenda');
        const connection = el('select', { class: 'input', 'aria-label': 'Conexão da Agenda' }, ...connections.filter(c => c.state === 'connected').map(c => el('option', { value: c.id, text: `${c.id} · ${c.scope}` })));
        const account = el('input', { class: 'input', 'aria-label': 'Conta no conector', placeholder: 'Apelido da conta, ex.: work' }), calendar = el('input', { class: 'input', 'aria-label': 'Calendário', value: 'primary' }), timezone = el('input', { class: 'input', 'aria-label': 'Fuso horário da Agenda', value: 'America/Sao_Paulo' }), review = X.check('Revisei a conexão e quero apenas consultas nesta Agenda.');
        if (!await X.dialog('Configurar Agenda', 'Use o conector Google Calendar revisado. Selecione uma conta específica; consultas não mesclam contas nem criam eventos.', [field('Contexto da Agenda', context), field('Conexão da Agenda', connection), field('Conta no conector', account), field('Calendário', calendar), field('Fuso horário da Agenda', timezone), review.row], 'Vincular Agenda') || source !== api.base()) return;
        await X.mutate(() => api.vincularAgenda({ scope: context.value, connection_id: connection.value, account: account.value, calendar_id: calendar.value, timezone: timezone.value, reviewed_read_only: review.input.checked }));
    }
    const scope = () => O.projects.current()?.id || null;
    function eventDetails(row) {
        const p = row.payload;
        const date = value => new Date(value).toLocaleString('pt-BR', { timeZone: p.timeZone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
        return [el('h4', { text: p.summary }), X.hint(`${p.account} · ${p.calendarId} · ${p.timeZone}`),
            X.hint(`${date(p.start)} até ${date(p.end)}`), el('p', { text: p.description }), X.hint(`Local: ${p.location || 'Não informado'}`), X.hint('Sem participantes ou convites.')];
    }
    async function review(row) {
        const project = scope(), source = api.base();
        if (!await X.dialog('Revisar evento', 'Confira exatamente o que será enviado. Pedir aprovação ainda não cria o evento.', eventDetails(row), 'Pedir aprovação') || project !== scope() || source !== api.base()) return;
        await X.mutate(() => api.revisarEvento(row.id, project, row.digest));
    }
    async function confirm(row, approval) {
        const project = scope(), source = api.base();
        if (!await X.dialog('Criar evento', 'Esta confirmação envia o evento à conta apresentada.', eventDetails(row), 'Confirmar criação') || project !== scope() || source !== api.base()) return;
        await X.mutate(async () => { if (approval.status !== 'approved') await api.decidir(approval.id, true); return api.criarEvento(row.id, project, approval.id); });
    }
    async function proposal() {
        if (!O.historico.sessao()) return O.ui.toast('Abra uma conversa neste contexto antes de preparar o evento.', { tipo: 'aviso' });
        const project = scope(), source = api.base(), session = O.historico.sessao();
        const inputs = {};
        const fields = [['title', 'Título'], ['start', 'Início com fuso'], ['end', 'Fim com fuso'], ['description', 'Descrição'], ['location', 'Local']].map(([key, name]) => {
            inputs[key] = el(key === 'description' ? 'textarea' : 'input', { class: 'input', 'aria-label': name, placeholder: key === 'start' || key === 'end' ? '2026-10-07T09:00:00-03:00' : '' }); return field(name, inputs[key]);
        });
        if (!await X.dialog('Propor evento', 'A proposta fica salva para revisão. Nenhum evento será criado nesta etapa.', fields, 'Salvar proposta') || project !== scope() || source !== api.base() || session !== O.historico.sessao()) return;
        await X.mutate(() => api.proporEvento(project, { session_id: session, ...Object.fromEntries(Object.entries(inputs).map(([key, input]) => [key, input.value])) }));
    }
    async function panel() {
        const rows = await api.agendas(), proposals = await api.propostasEventos(scope());
        return el('section', { class: 'calendar-section', 'aria-label': 'Agendas revisadas' }, el('h3', { text: 'Agenda' }), X.hint('Consulte compromissos e disponibilidade com conta, projeto e fuso explícitos.'), el('div', { class: 'extension-actions calendar-actions' }, X.button('Configurar Agenda', bind), X.button('Propor evento', proposal)), ...rows.map(row => X.hint(`${row.account} · ${row.calendar_id === 'primary' ? 'Calendário principal' : row.calendar_id} · ${row.timezone} · ${row.scope === 'personal' ? 'Pessoal' : (O.projects.options('Contexto', row.scope).selectedOptions[0]?.textContent || 'Projeto')}`)),
            ...proposals.map(row => { const approval = row.approvals[0]; return el('article', { class: 'card fact-item', dataset: { eventProposal: row.id } }, ...eventDetails(row),
                X.hint(row.status === 'created' ? 'Evento criado.' : row.status === 'unknown' || row.status === 'sending' ? 'Resultado ainda não confirmado. Confira a agenda antes de criar outra proposta.' : 'Proposta salva · nenhum evento criado.'),
                ['draft', 'pending'].includes(row.status) ? el('div', { class: 'extension-actions' }, approval ? X.button(approval.status === 'approved' ? 'Enviar evento aprovado' : 'Confirmar criação', () => confirm(row, approval)) : X.button('Revisar criação', () => review(row)), approval?.status === 'pending' ? X.button('Rejeitar criação', () => X.mutate(() => api.decidir(approval.id, false))) : null) : null); }));
    }
    O.calendar = { panel };
})();
