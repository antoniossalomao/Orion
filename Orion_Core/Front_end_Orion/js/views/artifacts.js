/* Biblioteca durável: prévia local, downloads autenticados e fontes da mensagem. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus, ui } = O;
    const names = { text: 'Texto', markdown: 'Markdown', code: 'Código', image: 'Imagem' };
    let active = false, generation = 0, selected = null, blobUrl = null, scopeKey = null, busy = false;
    const versionChoices = new Map(), drafts = new Map(), origin = () => `${api.base()}:${api.estado().backend}`, scope = () => O.projects.current()?.id || null;
    const button = (text, fn, cls = 'btn btn-outline btn-sm') => el('button', { type: 'button', class: cls, text, on: { click: fn } });
    const field = (label, input) => el('label', { class: 'extension-field' }, el('span', { text: label }), input);
    const hint = text => el('p', { class: 'extension-hint', text });
    function release() { O.htmlPreview.release(); if (blobUrl) URL.revokeObjectURL(blobUrl); blobUrl = null; }
    function close(focus = true) {
        ++generation; release(); const id = selected; selected = null;
        $('#artifact-preview').hidden = true; $('#artifact-preview').replaceChildren();
        if (focus && id) $(`#artifact-list [data-artifact="${CSS.escape(id)}"] button`)?.focus();
    }
    async function download(row) {
        try {
            const response = await api.baixarResultado(row.id, row.project_id, row.version), blob = await response.blob();
            const url = URL.createObjectURL(blob), link = el('a', { href: url });
            link.download = `${row.title}${row.kind === 'image' ? '.png' : row.kind === 'markdown' ? '.md' : '.txt'}`;
            document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
        } catch (error) { ui.toast(error.message, { tipo: 'erro' }); }
    }
    async function conversation(row) {
        try {
            const known = O.sidebar.sessoes().find(s => s.sessao_id === row.session_id);
            if (known) await O.sidebar.abrir(known);
            else { await O.historico.abrir(row.session_id, true); O.app.ir('chat'); }
            if (row.message_id) requestAnimationFrame(() => $(`.msg[data-message-id="${row.message_id}"]`)?.scrollIntoView({ block: 'center' }));
        } catch (error) { ui.toast(error.message, { tipo: 'erro' }); }
    }
    async function open(id, version = null, focus = true) {
        if (version === null && versionChoices.has(id)) version = versionChoices.get(id);
        const token = ++generation, context = scope(), source = origin(); selected = id;
        release(); const panel = $('#artifact-preview'); panel.hidden = false; panel.replaceChildren(hint('Abrindo resultado…'));
        try {
            const [row, versions] = await Promise.all([api.resultado(id, context, version), api.versoesResultado(id, context)]);
            if (token !== generation || context !== scope() || source !== origin()) return;
            versionChoices.set(id, row.version);
            const selector = el('select', { class: 'input', 'aria-label': 'Versão do resultado' }, ...versions.map(v => el('option', { value: String(v.version), text: `Versão ${v.version} · ${new Date(v.created_at * 1000).toLocaleDateString('pt-BR')}` })));
            selector.value = String(row.version); selector.addEventListener('change', () => open(id, Number(selector.value), false));
            const title = el('h3', { tabindex: '-1', text: row.title });
            const content = el('div', { class: 'artifact-content selectable' });
            if (row.kind === 'image') {
                const response = await api.baixarResultado(id, context, row.version), blob = await response.blob();
                if (token !== generation || context !== scope() || source !== origin()) return;
                blobUrl = URL.createObjectURL(blob); content.append(el('img', { src: blobUrl, alt: row.title, class: 'artifact-image' }));
            } else if (row.kind === 'markdown') {
                const template = document.createElement('template'); template.innerHTML = O.md.renderizar(row.content.slice(0, 120000));
                template.content.querySelectorAll('img').forEach(img => img.replaceWith(document.createTextNode('[Imagem incorporada bloqueada. Abra uma imagem salva na biblioteca.]')));
                content.classList.add('prose'); content.append(template.content);
            } else {
                if (row.kind === 'code' && row.language.toLowerCase() === 'html') content.append(O.htmlPreview.panel(row));
                content.append(el('pre', { class: 'artifact-code', text: row.content.slice(0, 120000) }));
            }
            if (row.content?.length > 120000) content.append(hint('Prévia abreviada. Baixe o conteúdo completo.'));
            const labels = [...(row.provenance?.memoria || []).map(p => p.fonte), ...(row.provenance?.skills || []).map(p => `${p.id} · ${p.version}`), ...(row.provenance?.atividades || []).map(p => `${p.display_name || p.name} · ${p.origin_label || p.origin || 'Orion'}`)].filter(Boolean);
            panel.replaceChildren(el('div', { class: 'artifact-preview-head' }, title, button('Fechar prévia', () => close(), 'btn btn-ghost btn-sm')),
                hint(`${names[row.kind]}${row.language ? ' · ' + row.language : ''} · ${row.session_title || 'Conversa sem título'}`), field('Versão do resultado', selector),
                el('div', { class: 'extension-actions' }, button('Baixar', () => download(row)), button('Abrir conversa', () => conversation(row)),
                    row.kind !== 'image' ? button('Criar nova versão', () => editor(row, versions[0].version)) : null),
                el('details', {}, el('summary', { text: 'Fontes e origem' }), hint(row.message_id ? `Mensagem ${row.message_id} da conversa original.` : 'Resultado salvo manualmente nesta conversa.'), ...labels.map(hint), hint(`Revisão ${row.digest.slice(0, 12)}`)), content);
            if (focus) title.focus({ preventScroll: true });
        } catch (error) { if (token === generation) panel.replaceChildren(hint(error.message), button('Fechar prévia', () => close())); }
    }
    async function editor(row = null, expectedVersion = null, responseText = '', messageId = null, originalSession = null) {
        if (busy || O.chat.ocupado()) { ui.toast('Espere a resposta terminar para salvar.', { tipo: 'aviso' }); return; }
        const source = origin(), context = scope(), sid = row?.session_id || originalSession || O.historico.sessao();
        if (!sid) { ui.toast('Abra uma conversa para associar o resultado.', { tipo: 'aviso' }); return; }
        const key = `${source}:${context}:${row?.id || sid}:${messageId || 'new'}`, draft = drafts.get(key) || row || {};
        const title = el('input', { class: 'input', type: 'text', maxlength: '160', value: draft.title || 'Resultado da conversa', 'aria-label': 'Título do resultado' });
        const kind = el('select', { class: 'input', 'aria-label': 'Tipo do resultado' }, ...Object.entries(names).filter(([k]) => k !== 'image').map(([value, text]) => el('option', { value, text })));
        kind.value = draft.kind || 'markdown'; kind.disabled = !!row;
        const content = el('textarea', { class: 'input artifact-editor', rows: '9', 'aria-label': 'Conteúdo do resultado', text: draft.content || responseText });
        const language = el('input', { class: 'input', 'aria-label': 'Linguagem do código', maxlength: '48', value: draft.language || '', placeholder: 'Ex.: html, python' });
        const languageField = field('Linguagem do código', language); languageField.hidden = kind.value !== 'code';
        kind.addEventListener('change', () => { languageField.hidden = kind.value !== 'code'; });
        const accepted = await O.extensions.dialog(row ? 'Criar nova versão' : 'Salvar resultado', 'O resultado fica associado a esta conversa. Você poderá reabrir, revisar e baixar depois.', [field('Título do resultado', title), field('Tipo do resultado', kind), languageField, field('Conteúdo do resultado', content)], row ? 'Salvar nova versão' : 'Salvar resultado');
        const data = { title: title.value.trim(), kind: kind.value, content: content.value, session_id: sid, message_id: row?.message_id || messageId, language: kind.value === 'code' ? language.value.trim() : '', expected_version: expectedVersion };
        drafts.set(key, data); if (!accepted || source !== origin() || context !== scope()) return;
        busy = true;
        try {
            const result = await api.salvarResultado(data, context, row?.id);
            if (source !== origin() || context !== scope()) return;
            drafts.delete(key); selected = result.id; versionChoices.set(result.id, result.version);
            if (O.app.view() !== 'resultados') O.app.ir('resultados');
            else { await load(); await open(result.id); }
        } catch (error) { if (source === origin()) ui.toast(`${error.message} Seu conteúdo foi preservado.`, { tipo: 'erro' }); }
        finally { busy = false; }
    }
    async function load() {
        const token = ++generation, context = scope(), source = origin();
        const status = $('#artifact-status');
        $('#artifact-new').disabled = !api.suporta('artifacts');
        if (!api.suporta('artifacts')) { status.textContent = 'Resultados ainda indisponíveis neste backend.'; return; }
        $('#artifact-context').textContent = context ? `Resultados de ${O.projects.current().name}` : 'Resultados pessoais · sem projeto';
        status.textContent = 'Carregando resultados…';
        try {
            const rows = await api.resultados(context, $('#artifact-search').value);
            if (token !== generation || context !== scope() || source !== origin()) return;
            $('#artifact-list').replaceChildren(...rows.map(row => el('article', { class: 'card artifact-item', dataset: { artifact: row.id } },
                button(row.title, () => open(row.id), 'btn btn-ghost artifact-title'), hint(`${names[row.kind]} · versão ${row.version}`))));
            status.textContent = rows.length ? `${rows.length} resultado${rows.length === 1 ? '' : 's'}` : 'Guarde uma resposta do chat ou crie seu primeiro resultado.';
        } catch (error) { if (token === generation) status.textContent = `Não consegui carregar: ${error.message}`; }
    }
    O.views.resultados = {
        init() {
            $('#artifact-new').addEventListener('click', () => editor()); $('#artifact-search').addEventListener('input', O.util.debounce(load, 250));
            $('#artifact-preview').addEventListener('click', e => { const link = e.target.closest('a[href]'); if (link) { e.preventDefault(); api.abrirExterno(link.href); } });
            bus.on('sessoes', () => { const key = `${origin()}:${scope()}`; if (scopeKey !== key) { scopeKey = key; close(false); if (active) load(); } });
            bus.on('capabilities', () => { close(false); $('#artifact-list').replaceChildren(); if (active) load(); });
        },
        async ativar() { active = true; await load(); if (active && selected) await open(selected, null, false); },
        desativar() { active = false; ++generation; release(); }, escape() { if (selected) { close(); return true; } return false; }
    };
    O.artifacts = { abrir: open, salvarResposta: (text, messageId, sessionId) => editor(null, null, text, messageId, sessionId) };
})();
