/* HTML estático: origem opaca, sem scripts, rede, formulários ou ponte desktop. */
(function () {
    'use strict';
    const O = window.Orion, { el } = O;
    const frames = new Map();
    const supported = () => O.api.suporta('html_preview') && !window.pywebview && 'sandbox' in document.createElement('iframe');
    function staticMarkup(content) {
        const template = document.createElement('template');
        template.innerHTML = content.slice(0, 120000);
        template.content.querySelectorAll('script,link,meta,base,iframe,object,embed').forEach(node => node.remove());
        for (const node of template.content.querySelectorAll('*')) {
            for (const attr of [...node.attributes]) {
                const name = attr.name.toLowerCase();
                if (name.startsWith('on') || ['src', 'srcset', 'action', 'formaction', 'poster', 'ping'].includes(name) || name.endsWith('href')) {
                    node.removeAttribute(attr.name);
                }
            }
        }
        return template.innerHTML;
    }
    function release() { for (const [frame, url] of frames) { frame.remove(); URL.revokeObjectURL(url); } frames.clear(); }
    function panel(row) {
        const root = el('div'), status = el('p', { class: 'extension-hint', role: 'status', dataset: { htmlPreviewStatus: '' }, text: supported() ? 'HTML estático · sem JavaScript, rede ou formulários. O código continua disponível.' : 'Prévia HTML indisponível neste ambiente. Consulte o código abaixo.' });
        const show = el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Prévia HTML', dataset: { htmlPreviewShow: '' } }), code = el('button', { class: 'btn btn-outline btn-sm', type: 'button', text: 'Ver código', dataset: { htmlPreviewCode: '' } });
        show.disabled = !supported(); code.hidden = true;
        function close() { release(); show.hidden = false; show.disabled = !supported(); code.hidden = true; }
        show.addEventListener('click', () => {
            if (!supported()) { show.disabled = true; return; }
            close();
            const csp = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";
            const html = `<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"></head><body>${staticMarkup(row.content)}</body></html>`;
            const url = URL.createObjectURL(new Blob([html], { type: 'text/html' }));
            const frame = el('iframe', { class: 'artifact-html-preview', title: `${row.title} · prévia HTML`, sandbox: '', referrerpolicy: 'no-referrer', src: url });
            frames.set(frame, url); root.append(frame); show.hidden = true; code.hidden = false;
        });
        code.addEventListener('click', close);
        root.append(status, el('div', { class: 'extension-actions' }, show, code));
        return root;
    }
    window.addEventListener('pywebviewready', () => {
        release();
        document.querySelectorAll('[data-html-preview-show]').forEach(button => { button.hidden = false; button.disabled = true; });
        document.querySelectorAll('[data-html-preview-code]').forEach(button => { button.hidden = true; });
        document.querySelectorAll('[data-html-preview-status]').forEach(status => { status.textContent = 'Prévia HTML indisponível no desktop. Consulte o código abaixo.'; });
    });
    O.htmlPreview = { panel, release };
})();
