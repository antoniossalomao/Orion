/* Fontes externas: progresso, retry e original preservado. O draft não é alterado. */
(function () {
    'use strict';
    const O = window.Orion, { $, el, api, bus } = O;
    let active = false, generation = 0, input, status, list, busy = false, progress;
    const scope = () => O.projects.current()?.id || null;
    const errors = { pdf_requires_ocr: 'Este PDF não contém texto legível. OCR ainda não está disponível.', pdf_encrypted: 'PDF protegido por senha.', pdf_page_limit: 'Limite de 200 páginas.', document_size_limit: 'Limite de 6 MB por arquivo.', document_invalid: 'Não consegui ler o documento. O original foi preservado.', document_text_limit: 'Texto vazio ou maior que 2 MB.', document_extraction_timeout: 'A extração demorou demais. Tente novamente.' };
    const button = (text, fn) => el('button', { class: 'btn btn-outline btn-sm', type: 'button', text, on: { click: fn } });
    async function operation(fn) {
        if (busy) return; busy = true; input.disabled = true; progress.hidden = false; progress.removeAttribute('value');
        const project = scope(), source = api.base();
        status.textContent = 'Enviando e extraindo texto…';
        try { const row = await fn(project); if (project === scope() && source === api.base()) { input.value = ''; await load(); status.textContent = row.status === 'ready' ? 'Fonte indexada neste contexto.' : (errors[row.error] || 'Falha na extração. Original preservado para baixar ou tentar novamente.'); } }
        catch (error) { if (project === scope() && source === api.base()) status.textContent = `Não foi possível enviar: ${error.message}. O arquivo selecionado e seu rascunho foram preservados.`; }
        finally { busy = false; input.disabled = false; progress.hidden = true; }
    }
    async function load() {
        const token = ++generation, project = scope(), source = api.base();
        if (!api.suporta('documents')) { status.textContent = 'Fontes ainda indisponíveis neste backend.'; return; }
        try {
            const rows = await api.documentos(project);
            if (token !== generation || project !== scope() || source !== api.base()) return;
            status.textContent = `${project ? O.projects.current().name : 'Pessoal'} · ${rows.length} documento(s)`;
            list.replaceChildren(...rows.map(row => el('article', { class: 'card fact-item', dataset: { documentId: row.id } },
                el('h3', { text: row.name }), el('p', { class: 'extension-hint', text: `${Math.ceil(row.size / 1024)} KB · ${row.status === 'ready' ? 'Indexado' : row.status === 'error' ? (errors[row.error] || 'Falha de extração') : 'Processamento interrompido'}` }),
                el('div', { class: 'extension-actions' }, row.status !== 'ready' ? button('Tentar novamente', () => operation(context => api.repetirDocumento(row.id, context))) : null,
                    button('Baixar original', async () => { try { const response = await api.baixarDocumento(row.id, project), url = URL.createObjectURL(await response.blob()), a = el('a', { href: url, download: row.name }); a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); } catch (error) { status.textContent = error.message; } })))));
        } catch (error) { if (token === generation) status.textContent = error.message; }
    }
    O.views.fontes = {
        init() {
            input = el('input', { class: 'input', type: 'file', accept: '.pdf,.txt,.md', 'aria-label': 'Adicionar documento' });
            status = el('p', { id: 'document-status', role: 'status' }); list = el('div'); progress = el('progress', { 'aria-label': 'Processando documento', max: '100' }); progress.hidden = true;
            $('#documents-root').append(el('div', { class: 'page-head' }, el('div', {}, el('h2', { text: 'Suas fontes' }), el('p', { text: 'PDF com texto, Markdown e TXT · até 6 MB e 200 páginas. Conteúdo tratado como fonte externa.' }))),
                el('label', { class: 'extension-field' }, el('span', { text: 'Adicionar documento' }), input), button('Enviar e indexar', () => { const file = input.files[0]; if (file) operation(project => api.ingerirDocumento(file, project)); }), progress, status, list);
            for (const event of ['sessoes', 'capabilities']) bus.on(event, () => { generation++; list.replaceChildren(); if (active && !busy) load(); });
        }, ativar() { active = true; load(); }, desativar() { active = false; generation++; }
    };
})();
