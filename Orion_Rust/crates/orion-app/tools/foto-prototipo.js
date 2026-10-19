// Tira as fotos do protótipo HTML (orion-ui-mockup.html) nas mesmas cenas de `src/snap.rs`.
// Uso: node foto-prototipo.js <orion-ui-mockup.html> <pasta-de-saida> [filtro]
// Requer playwright (npm i -g playwright) e o Chromium do Playwright.
const path = require('path');
const fs = require('fs');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const [html, out, filtro] = process.argv.slice(2);
const FONTS = path.join(__dirname, '..', 'assets', 'fonts');
const faces = [
  ['Geist', 400, 'normal', 'geist-sans-latin-400-normal'], ['Geist', 500, 'normal', 'geist-sans-latin-500-normal'], ['Geist', 600, 'normal', 'geist-sans-latin-600-normal'],
  ['Geist Mono', 400, 'normal', 'geist-mono-latin-400-normal'], ['Geist Mono', 500, 'normal', 'geist-mono-latin-500-normal'],
  ['Newsreader', 400, 'normal', 'newsreader-latin-400-normal'], ['Newsreader', 500, 'normal', 'newsreader-latin-500-normal'],
  ['Newsreader', 600, 'normal', 'newsreader-latin-600-normal'], ['Newsreader', 400, 'italic', 'newsreader-latin-400-italic'],
];
const css = faces.map(([n, w, s, f]) => `@font-face{font-family:"${n}";font-weight:${w};font-style:${s};src:url("file://${FONTS}/${f}.ttf")}`).join('');
const cenas = require('./cenas.js');

(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM, args: ['--allow-file-access-from-files'] });
  for (const c of cenas) {
    if (filtro && !c.nome.includes(filtro)) continue;
    const [w, h] = c.tamanho || [1440, 900];
    const p = await browser.newPage({ viewport: { width: w, height: h } });
    await p.clock.setFixedTime(new Date('2026-10-19T09:41:00'));
    await p.goto('file://' + path.resolve(html));
    await p.addStyleTag({ content: css });
    await p.waitForTimeout(300);
    if (c.nome !== '01-login') {
      await p.fill('#pw', 'x');
      await p.press('#pw', 'Enter');
      await p.waitForTimeout(3200); // some o toast "git pull do vault"
    }
    await c.rodar(p);
    await p.waitForTimeout(500);
    await p.screenshot({ path: path.join(out, c.nome + '.png') });
    console.log(c.nome);
    await p.close();
  }
  await browser.close();
})();
