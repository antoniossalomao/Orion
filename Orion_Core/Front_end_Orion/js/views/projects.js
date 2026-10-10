/* Projetos: contexto explícito, alterações administrativas e drafts por conversa. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus, ui } = O;
    let rows = [], selected = null, contextId = null, active = false, generation = 0, busy = false, contextButton;
    const drafts = new Map(), origin = () => `${api.base()}:${api.estado().backend}`;
    const button = (text, fn, cls = 'btn btn-outline btn-sm') => el('button', { type: 'button', class: cls, text, on: { click: fn } });
    const field = (label, input) => el('label', { class: 'extension-field' }, el('span', { text: label }), input);
    const hint = text => el('p', { class: 'extension-hint', text });
    const current = () => rows.find(row => row.id === contextId) || (contextId ? { id: contextId, name: 'Projeto' } : null);
    function options(label = 'Escopo da extensão', value = null) {
        const select = el('select', { class: 'input', 'aria-label': label }, el('option', { value: 'personal', text: 'Pessoal · sem projeto' }),
            ...rows.filter(r => !r.archived).map(r => el('option', { value: `project:${r.id}`, text: r.name })));
        select.value = value || (contextId ? `project:${contextId}` : 'personal');
        if (!select.value) select.value = 'personal';
        return select;
    }
    function paintContext() {
        if (!contextButton) return;
        contextButton.hidden = !api.suporta('projects');
        const row = current();
        contextButton.textContent = row ? `Projeto · ${row.name}` : 'Pessoal · sem projeto';
        contextButton.title = row ? 'Ver instruções e fontes deste projeto' : 'Selecionar um projeto';
    }
    async function operation(fn) {
        if (busy || O.chat.ocupado()) { ui.toast('Espere a operação atual terminar.', { tipo: 'aviso' }); return; }
        busy = true; const source = origin();
        try { await fn(source); } catch (error) { if (source === origin()) ui.toast(error.message, { tipo: 'erro', id: 'project-operation' }); }
        finally { busy = false; }
    }
    async function use(id) {
        await operation(async source => {
            const result = await api.usarProjeto(id);
            if (source !== origin()) return;
            contextId = id; O.app.ir('chat');
            await O.historico.abrir(result.session_id);
            await O.sidebar.carregar();
            paintContext(); bus.emit('skills:changed');
        });
    }
    async function edit(row = null) {
        const source = origin(), key = `${source}:${row?.id || 'new'}`, draft = drafts.get(key) || row || {};
        const name = el('input', { class: 'input', type: 'text', maxlength: '120', value: draft.name || '', 'aria-label': 'Nome do projeto' });
        const instructions = el('textarea', { class: 'input', rows: '5', maxlength: '16000', 'aria-label': 'Instruções do projeto', text: draft.instructions || '' });
        const root = el('input', { class: 'input', type: 'text', value: draft.root || '', 'aria-label': 'Pasta autorizada do projeto', placeholder: 'Caminho absoluto · opcional' });
        const share = O.extensions.check('Permitir leitura da memória pessoal neste projeto', !!draft.share_personal);
        const accepted = await O.extensions.dialog(row ? 'Editar projeto' : 'Novo projeto',
            'As conversas e fontes ficam neste contexto. Escolha explicitamente se deseja compartilhar sua memória pessoal.',
            [field('Nome do projeto', name), field('Instruções do projeto', instructions), field('Pasta autorizada do projeto', root), share.row], row ? 'Salvar projeto' : 'Criar projeto');
        const data = { name: name.value.trim(), instructions: instructions.value, root: root.value.trim() || null, share_personal: share.input.checked };
        drafts.set(key, data);
        if (!accepted || source !== origin()) return;
        if (!data.name) { ui.toast('Informe um nome. Seus dados foram preservados.', { tipo: 'aviso' }); return; }
        await operation(async () => {
            const result = row ? await api.editarProjeto(row.id, data) : await api.criarProjeto(data);
            if (source !== origin()) return;
            drafts.delete(key); selected = result.id; await load();
        });
    }
    async function move(id) {
        if (O.chat.ocupado()) return;
        await loadRows(); const source = origin(), select = options('Destino da conversa');
        if (!await O.extensions.dialog('Mover conversa', 'O histórico e o rascunho serão preservados. As próximas respostas usarão as fontes e permissões do destino.', [field('Destino da conversa', select)], 'Mover conversa') || source !== origin()) return;
        await operation(async () => {
            await api.moverConversa(id, select.value === 'personal' ? null : select.value.slice(8));
            if (source !== origin()) return;
            await O.sidebar.carregar(); bus.emit('skills:changed'); if (active) await load();
        });
    }
    /** Palavra a palavra sem acento nem caixa; devolve o projeto único que casa, ou o motivo de não haver. */
    function acharPorNome(nome) {
        const q = O.util.norm(nome);
        if (['nenhum', 'pessoal', 'nenhuma', 'sem'].includes(q)) return { id: null };
        const ativos = rows.filter(r => !r.archived);
        const exato = ativos.filter(r => O.util.norm(r.name) === q);
        const achados = exato.length ? exato : ativos.filter(r => O.util.norm(r.name).includes(q));
        if (achados.length === 1) return { id: achados[0].id, nome: achados[0].name };
        return { erro: achados.length ? `Mais de um projeto casa com “${nome}”: ${achados.map(r => r.name).join(', ')}.` : `Nenhum projeto se chama “${nome}”.` };
    }
    async function moverPorNome(nome) {
        const atual = O.sidebar.ativa();
        if (!atual) { ui.toast('Abra uma conversa primeiro.', { tipo: 'aviso' }); return; }
        if (atual.somente_leitura) { ui.toast('Esta conversa é somente leitura.', { tipo: 'aviso' }); return; }
        if (O.chat.ocupado()) { ui.toast('Espere a resposta terminar para mover a conversa.', { tipo: 'aviso' }); return; }
        await loadRows();
        const r = acharPorNome(nome || '');
        if (r.erro) { ui.toast(r.erro, { tipo: 'aviso', ms: 4200 }); return; }
        await operation(async source => {
            await api.moverConversa(atual.sessao_id, r.id);
            if (source !== origin()) return;
            await O.sidebar.carregar(); bus.emit('skills:changed');
            ui.toast(r.id ? `Conversa movida para “${r.nome}”.` : 'Conversa fora de qualquer projeto.', { tipo: 'ok' });
        });
    }
    function moverAtual() { const atual = O.sidebar.ativa(); if (atual) return move(atual.sessao_id); ui.toast('Abra uma conversa primeiro.', { tipo: 'aviso' }); }
    async function show(id, token = generation) {
        selected = id; if (!id) { $('#project-detail').replaceChildren(hint('Crie seu primeiro projeto para reunir conversas e fontes.')); return; }
        const row = await api.projeto(id);
        if (token !== generation || id !== selected) return;
        const actions = el('div', { class: 'extension-actions' },
            !row.archived ? button('Usar este projeto', () => use(row.id), 'btn btn-primary') : null,
            button('Editar', () => edit(row)), button(row.archived ? 'Restaurar projeto' : 'Arquivar projeto', () => operation(async () => {
                await api.editarProjeto(row.id, { archived: !row.archived });
                if (contextId === row.id && O.historico.sessao()) await O.historico.abrir(O.historico.sessao());
                await O.sidebar.carregar(); await load();
            })));
        const conversations = row.sessions.map(s => button(s.title || 'Conversa sem título', async () => {
            const known = O.sidebar.sessoes().find(item => item.sessao_id === s.id);
            await O.sidebar.abrir(known || { sessao_id: s.id, titulo: s.title, arquivada: !!s.archived, somente_leitura: !!s.archived || row.archived, project_id: row.id });
        }, 'btn btn-ghost project-conversation'));
        $('#project-detail').replaceChildren(el('h3', { text: row.name }),
            hint(row.share_personal ? 'Memória do projeto e memória pessoal compartilhada.' : 'Memória deste projeto · pessoal separada.'), actions,
            el('h4', { text: 'Instruções' }), el('p', { class: 'project-instructions selectable', text: row.instructions || 'Nenhuma instrução adicional.' }),
            el('h4', { text: 'Conversas' }), ...(conversations.length ? conversations : [hint('Suas conversas aparecerão aqui.')]),
            el('h4', { text: 'Fontes' }), ...(row.documents.length ? row.documents.map(d => hint(d.title)) : [hint('Nenhuma fonte adicionada.')]),
            el('h4', { text: 'Extensões' }), ...(row.extensions.length ? row.extensions.map(p => hint(`${p.name} · ${p.version}`)) : [hint('Nenhuma extensão ativa neste projeto.')]),
            button('Gerenciar extensões', () => O.app.ir('integracoes')));
        $('#project-list').querySelectorAll('button').forEach(b => b.setAttribute('aria-current', b.dataset.id === id ? 'true' : 'false'));
    }
    async function loadRows() {
        if (!api.suporta('projects') || !api.autenticado()) { rows = []; paintContext(); return; }
        const source = origin(); const values = await api.projetos();
        if (source !== origin()) return; rows = values; paintContext(); bus.emit('projetos', rows);
    }
    async function load() {
        const token = ++generation;
        const status = $('#project-status');
        if (!api.suporta('projects')) { status.textContent = 'Projetos ainda indisponíveis neste backend.'; $('#project-new').disabled = true; $('#project-personal').disabled = true; return; }
        $('#project-new').disabled = false; $('#project-personal').disabled = false;
        status.textContent = 'Carregando projetos…';
        try {
            await loadRows(); const visible = $('#project-archived').checked ? await api.projetos(true) : rows;
            if (token !== generation) return;
            $('#project-list').replaceChildren(...visible.map(row => {
                const b = button(row.name, () => show(row.id).catch(e => ui.toast(e.message, { tipo: 'erro' })), 'btn btn-ghost project-list-item'); b.dataset.id = row.id; return b;
            }));
            status.textContent = visible.length ? '' : 'Seu próximo trabalho pode começar aqui.';
            await show(visible.some(r => r.id === selected) ? selected : visible[0]?.id, token);
        } catch (error) { if (token === generation) status.textContent = `Não consegui carregar: ${error.message}`; }
    }
    O.views.projetos = {
        init() {
            $('#project-new').addEventListener('click', () => edit()); $('#project-personal').addEventListener('click', () => use(null)); $('#project-archived').addEventListener('change', load);
            contextButton = button('Pessoal · sem projeto', () => { selected = contextId; O.app.ir('projetos'); }, 'btn btn-ghost btn-sm project-context');
            contextButton.id = 'project-context'; $('#composer .composer-inner').prepend(contextButton);
            bus.on('sessoes', ({ lista, ativa }) => { const id = lista.find(s => s.sessao_id === ativa)?.project_id || null;
                if (contextId !== id) { contextId = id; bus.emit('skills:changed'); } paintContext(); });
            bus.on('capabilities', () => { ++generation; rows = []; contextId = null; selected = null; paintContext(); loadRows().catch(() => {}); if (active) load(); });
            loadRows().catch(() => {}); paintContext();
        },
        ativar() { active = true; load(); }, desativar() { active = false; ++generation; }
    };
    O.projects = { current, options, mover: move, moverAtual, moverPorNome, use, loadRows, lista: () => rows, disponivel: () => api.suporta('projects') };
})();
