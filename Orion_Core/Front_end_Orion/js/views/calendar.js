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
    async function panel() {
        const rows = await api.agendas();
        return el('section', { 'aria-label': 'Agendas revisadas' }, el('h3', { text: 'Agenda' }), X.hint('Consulte compromissos e disponibilidade com conta, projeto e fuso explícitos.'), X.button('Configurar Agenda', bind), ...rows.map(row => X.hint(`${row.account} · ${row.calendar_id} · ${row.timezone} · ${row.scope}`)));
    }
    O.calendar = { panel };
})();
