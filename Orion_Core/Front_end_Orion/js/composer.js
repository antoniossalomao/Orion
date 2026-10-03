/* ==========================================================================
   ORION — composer.js | caixa de mensagem (início e chat), anexos, modelo, histórico
   Enter envia · Shift+Enter quebra linha · ↑ repete o anterior · Esc para a resposta.
   No celular (ponteiro "grosso") Enter quebra a linha e o botão envia.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { $, $$, el, icone, bus, ui, api, prefs } = O;
    const U = O.util;

    const MODELOS = [
        { id: 'auto', nome: 'Automático', desc: 'Cascata Groq → Gemini → Claude' },
        { id: 'groq', nome: 'Groq', desc: 'O mais rápido' },
        { id: 'gemini', nome: 'Gemini', desc: 'Contexto longo' },
        { id: 'claude', nome: 'Claude', desc: 'Código e raciocínio' },
    ];
    const TIPOS_VIDEO = ['video/mp4', 'video/quicktime', 'video/webm'];
    const MAX_ANEXOS = 4;
    const MAX_ALTURA = 192;
    const CHAVE_RASCUNHO = 'orion_rascunho';

    let chat, inicio;               // instâncias {form, ta, enviar, caixa?}
    let ultimoPedido = null;        // {prompt, exibir, anexos, modelo}
    let anexos = [];                // {id, arquivo, estado, caminho, url, erro}
    const hist = { itens: [], idx: 0, rascunho: '', MAX: 50 };

    /* ── estado dos botões ─────────────────────────────────────────────── */
    const temTexto = inst => inst.ta.value.trim().length > 0;
    function atualizar() {
        const ocupado = O.chat.ocupado();
        const enviando = anexos.some(a => a.estado === 'enviando');
        const prontos = anexos.some(a => a.estado === 'ok');
        // chat: vira "parar" enquanto o Orion responde
        const parar = ocupado;
        chat.enviar.dataset.mode = parar ? 'stop' : 'send';
        chat.enviar.setAttribute('aria-label', parar ? 'Parar resposta' : 'Enviar');
        chat.enviar.dataset.tip = parar ? 'Parar (Esc)' : 'Enviar (Enter)';
        chat.enviar.querySelector('use').setAttribute('href', parar ? '#i-stop' : '#i-send');
        chat.enviar.disabled = parar ? false : !(temTexto(chat) || prontos) || enviando;
        inicio.enviar.disabled = ocupado || !temTexto(inicio);
        chat.ta.setAttribute('aria-busy', String(ocupado));
    }

    function autoajustar(ta) {
        ta.style.height = 'auto';
        ta.style.height = `${Math.min(ta.scrollHeight, MAX_ALTURA)}px`;
        ta.style.overflowY = ta.scrollHeight > MAX_ALTURA ? 'auto' : 'hidden';
    }

    /* ── rascunho (conveniência: sobrevive a recarregar a página) ─────── */
    const salvarRascunho = U.debounce(() => {
        try { chat.ta.value ? localStorage.setItem(CHAVE_RASCUNHO, chat.ta.value) : localStorage.removeItem(CHAVE_RASCUNHO); } catch (_) { /* storage bloqueado */ }
    }, 400);
    function restaurarRascunho() {
        try { const r = localStorage.getItem(CHAVE_RASCUNHO); if (r) { chat.ta.value = r; autoajustar(chat.ta); } } catch (_) { /* sem rascunho */ }
    }

    /* ── envio ─────────────────────────────────────────────────────────── */
    function montarPrompt(texto, prontos) {
        const linhas = prontos.map(a => (a.arquivo.type.startsWith('image/')
            ? `O que tem nessa imagem que anexei (use analisar_imagem): ${a.caminho}`
            : `Transcreva esse áudio/vídeo que anexei (use transcrever_audio): ${a.caminho}`));
        return [texto, linhas.join('\n')].filter(Boolean).join('\n\n');
    }

    function disparar({ prompt, exibir, nomes = [], semBolha = false }) {
        const modelo = prefs.get('model') || 'auto';
        ultimoPedido = { prompt, exibir, nomes, modelo };
        if (hist.itens[hist.itens.length - 1] !== exibir) { hist.itens.push(exibir); if (hist.itens.length > hist.MAX) hist.itens.shift(); }
        hist.idx = hist.itens.length;
        hist.rascunho = '';
        if (!semBolha) O.chat.usuario(exibir, { anexos: nomes });
        const via = O.transport.enviar({ texto: prompt, modelo });
        if (via === 'hub') O.chat.esperarEco(prompt);
        O.som?.envio?.();
    }

    function enviar(inst) {
        if (O.chat.ocupado()) {
            // Enter com a resposta em andamento NÃO a cancela (quem digita o próximo pedido não perde a resposta)
            ui.toast('Espere a resposta terminar, ou pare com Esc.', { tipo: 'aviso', ms: 2600, id: 'ocupado' });
            return;
        }
        const texto = inst.ta.value.trim();
        const prontos = inst === chat ? anexos.filter(a => a.estado === 'ok') : [];
        if (!texto && !prontos.length) return;
        if (anexos.some(a => a.estado === 'enviando')) { ui.toast('Aguarde o envio dos anexos terminar.', { tipo: 'aviso' }); return; }
        const nomes = prontos.map(a => a.arquivo.name);
        const prompt = montarPrompt(texto, prontos);
        inst.ta.value = '';
        autoajustar(inst.ta);
        if (inst === chat) { limparAnexos(false); salvarRascunho(); }
        if (document.documentElement.dataset.view !== 'chat') O.app.ir('chat');
        disparar({ prompt, exibir: texto || nomes.join(', '), nomes });
        atualizar();
        if (inst === chat) chat.ta.focus();
    }

    /* ── anexos ────────────────────────────────────────────────────────── */
    const aceita = f => /^(image|audio)\//.test(f.type) || TIPOS_VIDEO.includes(f.type);

    function desenharAnexos() {
        const linha = $('#attach-row');
        linha.hidden = anexos.length === 0;
        linha.replaceChildren(...anexos.map(a => {
            const midia = a.url ? el('img', { src: a.url, alt: '' })
                : el('span', { class: 'attach-ico', html: icone(a.arquivo.type.startsWith('image/') ? 'image' : 'file') });
            const estado = a.estado === 'enviando' ? 'enviando…' : a.estado === 'erro' ? (a.erro || 'falhou') : U.fmtBytes(a.arquivo.size);
            return el('div', { class: 'attach-chip', dataset: { estado: a.estado }, role: 'group', 'aria-label': `Anexo ${a.arquivo.name}, ${estado}` },
                midia, el('span', { class: 'name', text: a.arquivo.name, title: `${a.arquivo.name} · ${estado}` }),
                el('button', { class: 'remove', type: 'button', 'aria-label': `Remover anexo ${a.arquivo.name}`, html: icone('close'),
                    on: { click: () => removerAnexo(a.id) } }));
        }));
    }

    function removerAnexo(id) {
        const a = anexos.find(x => x.id === id);
        if (a?.url) URL.revokeObjectURL(a.url);
        anexos = anexos.filter(x => x.id !== id);
        desenharAnexos();
        atualizar();
        chat.ta.focus();
    }
    function limparAnexos(revogar = true) {
        if (revogar) anexos.forEach(a => a.url && URL.revokeObjectURL(a.url));
        anexos = [];
        desenharAnexos();
    }

    async function adicionar(arquivos) {
        const lista = Array.from(arquivos || []);
        if (!lista.length) return;
        if (document.documentElement.dataset.view !== 'chat') O.app.ir('chat');
        for (const f of lista) {
            if (!aceita(f)) { ui.toast(`"${f.name}": tipo não suportado. Use imagem, áudio ou vídeo (mp4, mov, webm).`, { tipo: 'aviso' }); continue; }
            if (anexos.length >= MAX_ANEXOS) { ui.toast(`No máximo ${MAX_ANEXOS} anexos por mensagem.`, { tipo: 'aviso' }); break; }
            const a = { id: U.uid('anx'), arquivo: f, estado: 'enviando', url: f.type.startsWith('image/') ? URL.createObjectURL(f) : '' };
            anexos.push(a);
            desenharAnexos();
            atualizar();
            api.upload(f).then(r => {
                if (!r || !r.ok || !r.path) throw new Error(r?.erro || 'o cérebro recusou o arquivo');
                a.estado = 'ok'; a.caminho = r.path;
            }).catch(e => {
                a.estado = 'erro'; a.erro = e.message;
                ui.toast(`Falha ao enviar "${f.name}": ${e.message}`, { tipo: 'erro' });
            }).finally(() => { desenharAnexos(); atualizar(); });
        }
        chat.ta.focus();
    }

    /* ── ligação de uma caixa ──────────────────────────────────────────── */
    function ligar(inst) {
        const { ta, form } = inst;
        ta.addEventListener('input', () => { autoajustar(ta); atualizar(); if (inst === chat) salvarRascunho(); });
        form.addEventListener('submit', e => { e.preventDefault(); enviar(inst); });
        ta.addEventListener('keydown', e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.isComposing && !O.toqueGrosso()) { e.preventDefault(); enviar(inst); return; }
            if (e.key === 'Escape' && O.chat.ocupado()) { e.preventDefault(); e.stopPropagation(); O.chat.parar(); return; }
            // ↑/↓ percorrem o que já foi enviado (campo vazio ou já navegando)
            if (e.key === 'ArrowUp' && hist.itens.length && (!ta.value || hist.idx < hist.itens.length) && ta.selectionStart === 0) {
                e.preventDefault();
                if (hist.idx === hist.itens.length) hist.rascunho = ta.value;
                if (hist.idx > 0) hist.idx--;
                ta.value = hist.itens[hist.idx];
                autoajustar(ta); atualizar();
            } else if (e.key === 'ArrowDown' && hist.idx < hist.itens.length && ta.selectionStart === ta.value.length) {
                e.preventDefault();
                hist.idx++;
                ta.value = hist.idx === hist.itens.length ? hist.rascunho : hist.itens[hist.idx];
                autoajustar(ta); atualizar();
            }
        });
        // colar print/imagem vira anexo
        ta.addEventListener('paste', e => {
            const arquivos = Array.from(e.clipboardData?.files || []).filter(aceita);
            if (arquivos.length) { e.preventDefault(); adicionar(arquivos); }
        });
        inst.enviar.addEventListener('click', e => {
            if (inst === chat && inst.enviar.dataset.mode === 'stop') { e.preventDefault(); O.chat.parar(); }
        });
    }

    /* ── modelo ────────────────────────────────────────────────────────── */
    function montarMenuModelo() {
        const menu = $('#model-menu');
        const desenhar = () => {
            const atual = prefs.get('model');
            const valido = MODELOS.some(m => m.id === atual) ? atual : 'auto';
            $('#model-label').textContent = MODELOS.find(m => m.id === valido).nome;
            menu.replaceChildren(...MODELOS.map(m => el('button', {
                class: 'menu-item', type: 'button', role: 'menuitemradio', 'aria-checked': String(m.id === valido), dataset: { valor: m.id },
            }, m.nome, m.desc ? el('small', { text: m.desc }) : null)));
        };
        desenhar();
        prefs.assinar('model', desenhar);
        // abre ACIMA da caixa de mensagem inteira: não cobre o clipe nem o texto que a pessoa digita
        const acima = () => {
            const dica = menu.parentElement.getBoundingClientRect(), caixa = $('#composer-box').getBoundingClientRect();
            menu.style.bottom = `${Math.max(8, dica.bottom - caixa.top + 8)}px`;
        };
        ui.ligarMenu($('#model-btn'), menu, { aoEscolher: v => prefs.set('model', v), aoAbrir: acima });
    }

    /* ── arrastar arquivos ─────────────────────────────────────────────── */
    function ligarArrastar() {
        let nivel = 0;
        const temArquivos = e => Array.from(e.dataTransfer?.types || []).includes('Files');
        const marcar = on => { chat.caixa.dataset.drag = String(on); };
        // sem isto o webview ABRE o arquivo solto e a interface some
        document.addEventListener('dragover', e => { e.preventDefault(); });
        document.addEventListener('dragenter', e => { e.preventDefault(); if (temArquivos(e)) { nivel++; marcar(true); } });
        document.addEventListener('dragleave', e => { e.preventDefault(); if (temArquivos(e) && --nivel <= 0) { nivel = 0; marcar(false); } });
        document.addEventListener('drop', e => { e.preventDefault(); nivel = 0; marcar(false); adicionar(e.dataTransfer?.files); });
    }

    /* ── API ───────────────────────────────────────────────────────────── */
    function foco() {
        const alvo = document.documentElement.dataset.view === 'home' ? inicio.ta : chat.ta;
        alvo.focus({ preventScroll: true });
    }
    function sugerir(texto) {
        if (O.chat.ocupado()) return;
        if (document.documentElement.dataset.view !== 'chat') O.app.ir('chat');
        disparar({ prompt: texto, exibir: texto });
        atualizar();
    }
    function reenviar() {
        if (!ultimoPedido || O.chat.ocupado()) return;
        const p = ultimoPedido;
        disparar({ prompt: p.prompt, exibir: p.exibir, nomes: p.nomes, semBolha: true });
        atualizar();
    }

    function init() {
        chat = { form: $('#composer'), ta: $('#composer-input'), enviar: $('#btn-send'), caixa: $('#composer-box') };
        inicio = { form: $('#home-form'), ta: $('#home-input'), enviar: $('#home-form .btn-send') };
        ligar(chat); ligar(inicio);
        $('#btn-attach').addEventListener('click', () => $('#file-input').click());
        $('#file-input').addEventListener('change', e => { adicionar(e.target.files); e.target.value = ''; });
        montarMenuModelo();
        ligarArrastar();
        restaurarRascunho();
        bus.on('chat:ocupado', atualizar);
        atualizar();
        // ao trocar o texto por fora (sugestões), reajusta a altura
        [chat, inicio].forEach(i => autoajustar(i.ta));
    }

    O.composer = { init, foco, sugerir, reenviar, adicionar, temPedido: () => !!ultimoPedido, MODELOS };
})();
