/* Extensões: catálogo revisável; o pacote só é ativado por decisão administrativa. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api } = O;
    const legacy = O.views.integracoes;
    const LABELS = { disabled: 'Desativado', active: 'Ativo', waiting_connection: 'Precisa de conexão', error: 'Revisar falha', installed: 'Instalado', connected: 'Conectada', configured: 'Configurada', failed: 'Falha na conexão', disconnected: 'Desconectada' };
    let tab = 'plugins', active = false, busy = false, generation = 0, tabs, panel;
    const origin = () => `${api.base()}:${api.estado().backend}`;
    const button = (text, fn, cls = 'btn btn-outline btn-sm') => el('button', { type: 'button', class: cls, text, on: { click: fn } });
    const hint = text => el('p', { class: 'extension-hint', text });
    const badge = state => el('span', { class: `badge ${state === 'active' || state === 'connected' ? 'badge-ok' : 'badge-muted'}`, text: LABELS[state] || state });

    function dialog(title, description, fields, accept = 'Confirmar') {
        return new Promise(resolve => {
            const previous = document.activeElement;
            const id = O.util.uid('extensions-dialog');
            const status = el('p', { role: 'status', class: 'extension-hint' });
            const ok = button(accept, () => finish(true), 'btn btn-primary');
            const box = el('div', { class: 'dialog extension-dialog', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': id },
                el('div', { class: 'dialog-head' }, el('h2', { id, text: title })),
                el('div', { class: 'dialog-body' }, hint(description), ...fields, status),
                el('div', { class: 'dialog-foot' }, button('Cancelar', () => finish(false), 'btn btn-ghost'), ok));
            const scrim = el('div', { class: 'dialog-scrim center', dataset: { open: 'false' } }, box);
            let finished = false;
            const release = O.ui.prenderFoco(box, () => finish(false));
            function finish(value) { if (finished) return; finished = true; release(); scrim.remove(); previous?.focus(); resolve(value); }
            $('#dialog-root').append(scrim);
            requestAnimationFrame(() => { scrim.dataset.open = 'true'; box.querySelector('input,select,button')?.focus(); });
        });
    }
    const check = (label, checked = false) => {
        const input = el('input', { type: 'checkbox' }); input.checked = checked;
        return { input, row: el('label', { class: 'extension-check' }, input, el('span', { text: label })) };
    };

    async function mutate(fn, focusId) {
        if (busy) return; busy = true;
        const trigger = document.activeElement; if (trigger?.tagName === 'BUTTON') trigger.disabled = true;
        const source = origin();
        try { await fn(); if (source !== origin()) return; await refresh();
            if (focusId) panel.querySelector(`[data-plugin="${CSS.escape(focusId)}"] button`)?.focus();
            O.bus.emit('skills:changed');
        } catch (error) { if (source === origin()) O.ui.toast(error.message, { tipo: 'erro', id: 'extension-operation' }); } finally { busy = false; if (trigger?.isConnected) trigger.disabled = false; }
    }
    async function review(row, version = null) {
        const source = origin(), target = version || { digest: row.selected_digest, capabilities: row.capabilities, version: row.version };
        const capabilities = target.capabilities.map(name => ({ name, ...check(name, !!version) }));
        const local = check('Confio no código local deste pacote e autorizo sua execução no computador.');
        const remote = check('Autorizo a conexão aos servidores remotos deste pacote.');
        const classifications = {};
        const details = [];
        if (!version) for (const connection of row.connections || []) {
            const fields = [];
            classifications[connection.id] = {};
            for (const name of Object.keys(connection.tools)) {
                const select = el('select', { class: 'input', 'aria-label': `Permissão para ${connection.id}/${name}` },
                    ...[['', 'Não conceder'], ['read', 'Leitura'], ['write', 'Escrita com revisão'], ['exec', 'Execução com revisão'], ['destructive', 'Remoção com revisão']].map(([value, text]) => el('option', { value, text })));
                classifications[connection.id][name] = select;
                fields.push(el('label', { class: 'extension-field' }, el('span', { text: name }), select));
            }
            details.push(el('details', {}, el('summary', { text: `Permissões do servidor ${connection.id}` }), ...fields));
        }
        const accepted = await dialog(version ? 'Revisar troca de versão' : `Ativar ${row.name}`,
            `Versão ${target.version} · revisão ${target.digest.slice(0, 12)}. Conceda apenas o que deseja usar. Cada ação de escrita ou execução ainda passa pela política do Orion.`,
            [hint('Capacidades solicitadas'), ...capabilities.map(c => c.row), ...details,
                ...(!version && row.connections?.some(c => c.transport === 'stdio') ? [local.row] : []),
                ...(!version && row.connections?.some(c => c.transport === 'http') ? [remote.row] : [])], version ? 'Usar esta versão' : 'Ativar');
        if (!accepted || source !== origin()) return;
        const reviewed = {};
        for (const [connection, names] of Object.entries(classifications)) {
            reviewed[connection] = Object.fromEntries(Object.entries(names).filter(([, input]) => input.value).map(([name, input]) => [name, input.value]));
        }
        await mutate(() => api.plugin(row.id, version ? 'version' : 'activate', {
            digest: target.digest, capabilities: capabilities.filter(c => c.input.checked).map(c => c.name),
            trusted_local: local.input.checked, authorized_remote: remote.input.checked, classifications: reviewed,
        }), row.id);
    }

    function pluginCard(row) {
        const actions = el('div', { class: 'extension-actions' });
        actions.append(row.state === 'active' ? button('Desativar', () => mutate(() => api.plugin(row.id, 'deactivate'), row.id)) : button('Ativar', () => review(row), 'btn btn-primary btn-sm'));
        actions.append(button('Versões', async () => {
            const source = origin();
            try {
                const versions = await api.versoesPlugin(row.id);
                if (source !== origin()) return;
                const select = el('select', { class: 'input', 'aria-label': 'Versão do plugin' }, ...versions.map(v => el('option', { value: v.digest, text: `${v.version}${v.digest === row.selected_digest ? ' · atual' : ''}` })));
                if (row.previous_digest) select.value = row.previous_digest;
                if (await dialog(`Versões de ${row.name}`, 'Trocar a versão desativa o plugin e revoga aprovações anteriores. Seus resultados são preservados.', [select], 'Revisar versão')) {
                    const target = versions.find(v => v.digest === select.value);
                    if (target && source === origin()) await review(row, target);
                }
            } catch (error) { O.ui.toast(error.message, { tipo: 'erro' }); }
        }));
        actions.append(button('Remover', async () => {
            const source = origin();
            if (await O.ui.confirmar({ titulo: `Remover ${row.name}?`, texto: 'O pacote será desativado e removido. Conversas e arquivos produzidos serão preservados. A autorização de contas é gerenciada separadamente.', ok: 'Remover', perigo: true }) && source === origin()) await mutate(() => api.plugin(row.id, ''), row.id);
        }, 'btn btn-ghost btn-sm'));
        return el('article', { class: 'card extension-card', dataset: { plugin: row.id } },
            el('div', { class: 'extension-card-head' }, el('h3', { text: row.name }), badge(row.state)),
            hint(row.description), el('div', { class: 'extension-meta', text: `v${row.version} · ${row.license} · ${row.origin === 'local' ? 'Pasta local' : 'Pacote importado'}` }),
            row.error ? el('p', { class: 'banner banner-warn', text: 'A ativação falhou. Confira a conexão e revise as permissões antes de tentar novamente.' }) : null,
            el('details', {}, el('summary', { text: 'Detalhes e capacidades' }),
                hint(`Identidade: ${row.id} · Revisão: ${row.selected_digest.slice(0, 16)}`),
                ...row.capabilities.map(text => el('p', { class: 'mono extension-capability', text }))), actions);
    }

    async function importFile() {
        const source = origin();
        const input = el('input', { type: 'file', accept: '.zip,application/zip' });
        input.addEventListener('change', () => { const file = input.files[0]; if (file && source === origin()) {
            if (file.size > 16000000) { O.ui.toast('O pacote deve ter até 16 MB.', { tipo: 'erro' }); return; }
            mutate(() => api.importarPlugin(file));
        } }, { once: true }); input.click();
    }

    async function refresh() {
        if (!active || tab === 'channels') return;
        const current = ++generation, source = origin(), selected = tab;
        panel.setAttribute('aria-busy', 'true');
        try {
            const rows = selected === 'plugins' ? await api.plugins() : selected === 'skills' ? await api.skills() : await api.conexoesMcp();
            if (current !== generation || source !== origin() || selected !== tab) return;
            const nodes = [];
            if (selected === 'plugins') {
                nodes.push(el('div', { class: 'extension-toolbar' }, hint('Instale capacidades novas e escolha quando ativá-las.'), button('Importar pacote ZIP', importFile, 'btn btn-primary btn-sm')));
                const folder = button('Instalar por pasta', async () => { const source = origin(); const path = await O.ui.confirmar({ titulo: 'Instalar pacote local', texto: 'Informe a pasta que contém manifest.json. O código não será executado durante a instalação.', campo: { rotulo: 'Caminho absoluto da pasta', limite: 4096 }, ok: 'Instalar' }); if (path && source === origin()) await mutate(() => api.instalarPlugin(path)); });
                nodes.push(el('details', { class: 'extension-local' }, el('summary', { text: 'Pacote de desenvolvimento' }), folder));
                nodes.push(el('div', { class: 'extension-grid' }, ...rows.map(pluginCard)));
            } else if (selected === 'skills') {
                nodes.push(hint('Instruções que ajudam o Orion a realizar tarefas. Ative o plugin de origem para usar uma skill.'));
                nodes.push(el('div', { class: 'extension-grid' }, ...rows.map(row => el('article', { class: 'card extension-card' },
                    el('div', { class: 'extension-card-head' }, el('h3', { text: row.name }), badge(row.enabled ? 'active' : 'disabled')),
                    hint(row.description), el('p', { class: 'extension-meta', text: `${row.id} · ${row.origin} · ${row.version}` }),
                    row.enabled ? button('Usar no chat', async () => { O.app.ir('chat'); await O.composer.carregarSkills(); O.composer.selecionarSkill(row.id); }) : null))));
            } else {
                nodes.push(hint('Conecte ferramentas e fontes de dados. Uma conexão não autoriza ações por conta própria.'));
                if (O.extensions?.connectionForm) nodes.push(button('Adicionar conexão', O.extensions.connectionForm, 'btn btn-primary btn-sm'));
                nodes.push(el('div', { class: 'extension-grid' }, ...rows.map(row => O.extensions?.connectionCard ? O.extensions.connectionCard(row) : el('article', { class: 'card extension-card' }, el('h3', { text: row.id }), badge(row.state), hint(row.code || row.error || '')))));
            }
            if (!rows.length) nodes.push(el('div', { class: 'extension-empty' }, el('h3', { text: selected === 'plugins' ? 'Seu Orion pode ir além' : selected === 'skills' ? 'Suas skills aparecerão aqui' : 'Nenhuma conexão configurada' }), hint(selected === 'plugins' ? 'Importe um pacote para pesquisar, organizar ou trabalhar com novas fontes. Você escolhe as permissões.' : 'Comece instalando um plugin ou configurando uma fonte.')));
            panel.replaceChildren(...nodes);
        } catch (error) {
            if (current === generation && source === origin()) panel.replaceChildren(el('p', { class: 'banner banner-warn', role: 'status', text: error.message }), button('Tentar novamente', refresh));
        } finally { if (current === generation) panel.setAttribute('aria-busy', 'false'); }
    }
    function select(id) {
        tab = id; generation++;
        for (const item of tabs.children) { const chosen = item.dataset.tab === tab; item.setAttribute('aria-selected', chosen); item.tabIndex = chosen ? 0 : -1; }
        panel.setAttribute('aria-labelledby', `extension-tab-${tab}`);
        panel.hidden = tab === 'channels'; $('#integ-grid').hidden = tab !== 'channels';
        $('#integ-refresh').hidden = tab !== 'channels';
        if (tab === 'channels') legacy.ativar(); else { legacy.desativar(); refresh(); }
    }
    function enabled() { return api.suporta('plugins') || api.suporta('mcp'); }
    O.extensions = { dialog, check, button, hint, badge, mutate, refresh };
    O.views.integracoes = {
        init() {
            legacy.init();
            tabs = el('div', { class: 'extension-tabs', role: 'tablist', 'aria-label': 'Tipos de extensão' });
            for (const [id, text] of [['plugins', 'Plugins'], ['skills', 'Skills'], ['mcp', 'Conexões MCP'], ['channels', 'Voz e canais']]) {
                const item = button(text, () => select(id), 'extension-tab');
                Object.assign(item.dataset, { tab: id }); item.id = `extension-tab-${id}`; item.setAttribute('role', 'tab'); item.setAttribute('aria-controls', id === 'channels' ? 'integ-grid' : 'extension-panel'); tabs.append(item);
            }
            tabs.addEventListener('keydown', event => {
                const ids = ['plugins', 'skills', 'mcp', 'channels'], index = ids.indexOf(tab);
                const next = event.key === 'ArrowRight' ? (index + 1) % 4 : event.key === 'ArrowLeft' ? (index + 3) % 4 : event.key === 'Home' ? 0 : event.key === 'End' ? 3 : null;
                if (next === null) return; event.preventDefault(); select(ids[next]); tabs.children[next].focus();
            });
            panel = el('div', { id: 'extension-panel', role: 'tabpanel', tabindex: '0' });
            $('#extension-root').append(tabs, panel);
        },
        async ativar() {
            active = true; await api.detectar();
            if (!active) return;
            const modern = enabled(); $('#extension-root').hidden = !modern;
            $('#tb-title').textContent = modern ? 'Extensões' : 'Integrações'; document.title = `${modern ? 'Extensões' : 'Integrações'} · Orion`;
            $('#view-integracoes h2').textContent = modern ? 'Extensões' : 'Integrações';
            const nav = $('.sb-item[data-view="integracoes"]'); nav.setAttribute('aria-label', modern ? 'Extensões' : 'Integrações'); nav.querySelector('.sb-txt').textContent = modern ? 'Extensões' : 'Integrações';
            $('#view-integracoes').setAttribute('aria-label', modern ? 'Extensões' : 'Integrações');
            $('#view-integracoes .page-head p').textContent = modern ? 'Dê novas capacidades ao Orion, com permissões sob seu controle.' : 'Canais e serviços ligados ao Orion.';
            if (modern) select(tab); else { $('#integ-grid').hidden = false; $('#integ-refresh').hidden = false; legacy.ativar(); }
        },
        desativar() { active = false; generation++; legacy.desativar(); },
    };
})();
