/* ==========================================================================
   ORION — interface desktop (pywebview + Three.js r128, tudo local)
   ==========================================================================
   Centro: a constelação de Órion em 3D, com as posições reais das estrelas
   e profundidade proporcional à distância de cada uma. Ela reage ao estado:
     em espera  → oscila devagar (a profundidade aparece no movimento)
     ouvindo    → o cinturão acende com a intensidade do áudio
     processando→ um traço percorre as linhas da figura
     respondendo→ todas as estrelas pulsam com a voz
   Eventos chegam pelo hub WS :8765 (orion_app.py); o chat sai pela API do
   pywebview (process_command) e a voz ao vivo fala direto com /ws/voice.
   ========================================================================== */

const PI = Math.PI;
const CEREBRO  = 'http://127.0.0.1:8000';
const HUB_WS   = 'ws://127.0.0.1:8765';        // 127.0.0.1, nunca localhost (IPv6 custa ~2s no Windows)
const VOICE_WS = 'ws://127.0.0.1:8000/ws/voice';

const COR = {
    idle:       '#7c9cff',
    listening:  '#5ee6c3',
    processing: '#f2b45a',
    speaking:   '#e8eeff',
    ok:         '#5ee6c3',
    off:        '#ff5f6d',
};
const ROTULO = {
    idle:       'Em espera',
    listening:  'Ouvindo',
    processing: 'Processando',
    speaking:   'Respondendo',
};

/* Preferências locais (só conveniência de UI; o estado real vive no backend) */
const Prefs = {
    get(k, padrao = null) {
        try { const v = localStorage.getItem('orion_' + k); return v === null ? padrao : v; }
        catch (_) { return padrao; }
    },
    set(k, v) { try { localStorage.setItem('orion_' + k, v); } catch (_) {} },
};

const _mqReduzido = window.matchMedia?.('(prefers-reduced-motion: reduce)');
function movimentoReduzido() {
    const p = Prefs.get('movimento_reduzido');
    if (p !== null) return p === '1';
    return !!_mqReduzido?.matches;
}

async function _fetchTimeout(url, ms, opts = {}) {
    const ctrl = new AbortController();
    const tid  = setTimeout(() => ctrl.abort(), ms);
    try { return await fetch(url, { ...opts, signal: ctrl.signal }); }
    finally { clearTimeout(tid); }
}

function _escapar(s) {
    return String(s).replace(/[&<>"']/g, c =>
        ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function _hora(d = new Date()) {
    return d.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
}

/* ==========================================================================
   CÉU — a constelação
   ========================================================================== */
const Ceu = (() => {
    // [id, ascensão reta (h), declinação (°), magnitude, distância (anos-luz), cor]
    const ESTRELAS = [
        ['betelgeuse', 5.9195,  7.4071, 0.50,  548, 0xffb27a],
        ['rigel',      5.2423, -8.2016, 0.13,  863, 0xc4d6ff],
        ['bellatrix',  5.4189,  6.3497, 1.64,  250, 0xcfdcff],
        ['mintaka',    5.5334, -0.2991, 2.23, 1200, 0xd6e2ff],
        ['alnilam',    5.6036, -1.2019, 1.69, 1340, 0xd0deff],
        ['alnitak',    5.6793, -1.9426, 1.77, 1260, 0xd3e0ff],
        ['saiph',      5.7959, -9.6696, 2.06,  650, 0xc9d8ff],
        ['meissa',     5.5856,  9.9342, 3.39, 1100, 0xd8e4ff],
        // espada
        ['c42',        5.5906, -4.8384, 4.59, 1300, 0xe0e8ff],
        ['m42',        5.5881, -5.3911, 4.00, 1344, 0xffe6f4],
        ['hatysa',     5.5904, -5.9100, 2.77, 1300, 0xdbe5ff],
        // escudo
        ['pi1',        4.9084, 10.1508, 4.65,  120, 0xffffff],
        ['pi2',        4.8438,  8.9002, 4.36,  190, 0xffffff],
        ['pi3',        4.8306,  6.9613, 3.19,   26, 0xfff4e0],
        ['pi4',        4.8535,  5.6050, 3.69, 1050, 0xd8e4ff],
        ['pi5',        4.9042,  2.4407, 3.72, 1340, 0xd8e4ff],
        ['pi6',        4.9761,  1.7140, 4.47,  950, 0xffe0c0],
        // clava
        ['mu',         6.0396,  9.6473, 4.12,  150, 0xffffff],
        ['nu',         6.1262, 14.7684, 4.42,  520, 0xd8e4ff],
        ['xi',         6.1966, 14.2088, 4.45,  630, 0xd8e4ff],
        ['chi1',       5.9064, 20.2762, 4.39,   28, 0xfff6e8],
        ['chi2',       6.0650, 20.1385, 4.63, 4900, 0xdbe5ff],
    ];
    // [a, b, peso] — peso 1 = figura principal; menor = escudo, clava, espada
    const LINHAS = [
        ['meissa', 'betelgeuse', 1], ['meissa', 'bellatrix', 1],
        ['betelgeuse', 'alnitak', 1], ['bellatrix', 'mintaka', 1],
        ['mintaka', 'alnilam', 1], ['alnilam', 'alnitak', 1],
        ['alnitak', 'saiph', 1], ['mintaka', 'rigel', 1],
        ['bellatrix', 'pi3', 0.5],
        ['pi1', 'pi2', 0.45], ['pi2', 'pi3', 0.45], ['pi3', 'pi4', 0.45],
        ['pi4', 'pi5', 0.45], ['pi5', 'pi6', 0.45],
        ['betelgeuse', 'mu', 0.5], ['mu', 'nu', 0.45], ['nu', 'xi', 0.45],
        ['nu', 'chi1', 0.45], ['chi1', 'chi2', 0.45],
        ['c42', 'm42', 0.4], ['m42', 'hatysa', 0.4],
    ];
    // Percurso do traço no estado "processando"
    const TRAJETO = ['meissa', 'betelgeuse', 'alnitak', 'saiph', 'alnitak', 'alnilam',
                     'mintaka', 'rigel', 'mintaka', 'bellatrix', 'meissa'];
    const CINTURAO = new Set(['alnitak', 'alnilam', 'mintaka']);

    const RA0 = 5.515, DEC0 = 5.3, ESCALA = 2.05;
    const VEL_TRACO = 15;            // unidades/s

    let scene, camera, renderer, root, fundo, cometa, nucleo, container;
    let estado = 'idle', alvoAudio = 0, audio = 0, flash = 1;
    let revelacao = -1;              // segundos desde o início da revelação; -1 = apagada
    let tracoS = 0, tracoOp = 0, segAtual = -1;
    let reduzido = movimentoReduzido();
    let ultimo = performance.now();
    let ok = false;
    const estrelas = new Map();
    const linhas = [];
    const trajeto = [];              // [{a, b, len, ia, ib}]
    let trajetoLen = 0;
    const _tmp = new THREE.Color();
    const TINT = {
        idle: null,
        listening:  new THREE.Color(COR.listening),
        processing: new THREE.Color(COR.processing),
        speaking:   new THREE.Color(COR.speaking),
    };
    const COR_LINHA = new THREE.Color(0x7c9cff);

    /* Texturas desenhadas em canvas — nada de arquivo externo */
    function _texRadial(paradas, sz = 64) {
        const cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const gr = ctx.createRadialGradient(sz / 2, sz / 2, 0, sz / 2, sz / 2, sz / 2);
        for (const [p, c] of paradas) gr.addColorStop(p, c);
        ctx.fillStyle = gr;
        ctx.fillRect(0, 0, sz, sz);
        return new THREE.CanvasTexture(cv);
    }
    const texEstrela = _texRadial([[0, 'rgba(255,255,255,1)'], [0.18, 'rgba(255,255,255,0.85)'],
                                   [0.45, 'rgba(255,255,255,0.16)'], [1, 'rgba(255,255,255,0)']]);
    const texHalo = _texRadial([[0, 'rgba(255,255,255,0.55)'], [0.35, 'rgba(255,255,255,0.12)'],
                                [1, 'rgba(255,255,255,0)']], 128);
    const texFlare = (() => {
        const sz = 256, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const raio = (horizontal) => {
            const gr = horizontal ? ctx.createLinearGradient(0, 0, sz, 0) : ctx.createLinearGradient(0, 0, 0, sz);
            gr.addColorStop(0, 'rgba(255,255,255,0)');
            gr.addColorStop(0.5, 'rgba(255,255,255,0.9)');
            gr.addColorStop(1, 'rgba(255,255,255,0)');
            ctx.fillStyle = gr;
            if (horizontal) ctx.fillRect(0, sz / 2 - 1, sz, 2); else ctx.fillRect(sz / 2 - 1, 0, 2, sz);
        };
        raio(true); raio(false);
        return new THREE.CanvasTexture(cv);
    })();
    function _texNebulosa(rgb, manchas = 5) {
        const sz = 256, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        for (let i = 0; i < manchas; i++) {
            const x = sz * (0.32 + Math.random() * 0.36), y = sz * (0.32 + Math.random() * 0.36);
            const r = sz * (0.18 + Math.random() * 0.2);
            const gr = ctx.createRadialGradient(x, y, 0, x, y, r);
            gr.addColorStop(0, `rgba(${rgb},0.5)`);
            gr.addColorStop(0.55, `rgba(${rgb},0.14)`);
            gr.addColorStop(1, `rgba(${rgb},0)`);
            ctx.fillStyle = gr;
            ctx.fillRect(0, 0, sz, sz);
        }
        return new THREE.CanvasTexture(cv);
    }

    function _sprite(tex, cor, op, escala, aditivo = true) {
        const s = new THREE.Sprite(new THREE.SpriteMaterial({
            map: tex, color: cor, transparent: true, opacity: op, depthWrite: false,
            blending: aditivo ? THREE.AdditiveBlending : THREE.NormalBlending,
        }));
        s.scale.set(escala, escala, 1);
        return s;
    }

    function _projetar(ra, dec, dist) {
        const x = -(ra - RA0) * 15 * Math.cos(DEC0 * PI / 180) * ESCALA;
        const y = (dec - DEC0) * ESCALA;
        const z = Math.max(-12, Math.min(12, -Math.log(dist / 700) * 9));
        return new THREE.Vector3(x, y, z);
    }

    function _construirFigura() {
        root = new THREE.Group();
        root.position.y = 3;
        scene.add(root);

        const porBrilho = [...ESTRELAS].sort((a, b) => a[3] - b[3]).map(e => e[0]);
        for (const [id, ra, dec, mag, dist, cor] of ESTRELAS) {
            const pos = _projetar(ra, dec, dist);
            const c = new THREE.Color(cor);
            const tam = 0.9 + (4.7 - mag) * 0.62;
            const g = new THREE.Group();
            g.position.copy(pos);
            const halo = _sprite(texHalo, c.clone(), 0, tam * 3.2);
            const nucleoEstrela = _sprite(texEstrela, c.clone(), 0, tam);
            g.add(halo); g.add(nucleoEstrela);
            let flare = null;
            if (mag < 0.6) { flare = _sprite(texFlare, c.clone(), 0, tam * 5.5); g.add(flare); }
            if (id === 'm42') {
                // Nebulosa de Órion: brilho difuso rosado na espada
                const neb = _sprite(_texNebulosa('255,120,190', 4), 0xffffff, 0, 9);
                g.add(neb);
                g.userData.neb = neb;
            }
            root.add(g);
            estrelas.set(id, {
                id, pos, cor: c, tam, op: Math.min(1, 0.55 + (4.7 - mag) * 0.11),
                fase: Math.random() * PI * 2, ritmo: 0.8 + Math.random() * 1.6,
                ordem: porBrilho.indexOf(id), cinturao: CINTURAO.has(id),
                grupo: g, halo, nucleo: nucleoEstrela, flare,
            });
        }

        LINHAS.forEach(([a, b, peso], i) => {
            const pa = estrelas.get(a).pos, pb = estrelas.get(b).pos;
            const geo = new THREE.BufferGeometry().setFromPoints([pa.clone(), pa.clone()]);
            const mat = new THREE.LineBasicMaterial({
                color: COR_LINHA.clone(), transparent: true, opacity: 0,
                blending: THREE.AdditiveBlending, depthWrite: false,
            });
            const linha = new THREE.Line(geo, mat);
            linha.frustumCulled = false;   // a geometria cresce durante a revelação
            root.add(linha);
            linhas.push({ a, b, pa, pb, base: 0.42 * peso, mat, geo, ordem: i, pronta: false,
                          cinturao: CINTURAO.has(a) && CINTURAO.has(b) });
        });

        for (let i = 0; i < TRAJETO.length - 1; i++) {
            const a = estrelas.get(TRAJETO[i]).pos, b = estrelas.get(TRAJETO[i + 1]).pos;
            const len = a.distanceTo(b);
            const idx = linhas.findIndex(l =>
                (l.a === TRAJETO[i] && l.b === TRAJETO[i + 1]) || (l.b === TRAJETO[i] && l.a === TRAJETO[i + 1]));
            trajeto.push({ a, b, len, linha: idx });
            trajetoLen += len;
        }

        // Traço: cabeça + rastro curto
        cometa = new THREE.Group();
        for (let k = 0; k < 7; k++) {
            const s = _sprite(texEstrela, new THREE.Color(COR.processing), 0, k === 0 ? 2.6 : 2.0 - k * 0.2);
            s.userData.k = k;
            cometa.add(s);
        }
        root.add(cometa);

        // Brilho de fundo atrás do cinturão
        nucleo = _sprite(texHalo, new THREE.Color(COR.idle), 0, 16);
        nucleo.position.copy(estrelas.get('alnilam').pos);
        nucleo.position.z -= 6;
        root.add(nucleo);
    }

    function _construirFundo() {
        fundo = new THREE.Group();
        const camadas = [
            { n: 160,  size: 2.2,  op: 0.7 },
            { n: 600,  size: 1.4,  op: 0.45 },
            { n: 1200, size: 0.9,  op: 0.3 },
        ];
        const tons = [new THREE.Color(0xdfe7ff), new THREE.Color(0xbfd0ff), new THREE.Color(0xffe9d2)];
        for (const L of camadas) {
            const pos = new Float32Array(L.n * 3), cor = new Float32Array(L.n * 3);
            for (let i = 0; i < L.n; i++) {
                const th = Math.random() * PI * 2, ph = Math.acos(2 * Math.random() - 1);
                const r = 260 + Math.random() * 160;
                pos[i * 3] = r * Math.sin(ph) * Math.cos(th);
                pos[i * 3 + 1] = r * Math.sin(ph) * Math.sin(th);
                pos[i * 3 + 2] = r * Math.cos(ph);
                const t = tons[(Math.random() * tons.length) | 0];
                cor[i * 3] = t.r; cor[i * 3 + 1] = t.g; cor[i * 3 + 2] = t.b;
            }
            const geo = new THREE.BufferGeometry();
            geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
            geo.setAttribute('color', new THREE.BufferAttribute(cor, 3));
            fundo.add(new THREE.Points(geo, new THREE.PointsMaterial({
                size: L.size, map: texEstrela, vertexColors: true, transparent: true, opacity: L.op,
                depthWrite: false, blending: THREE.AdditiveBlending, sizeAttenuation: true, alphaTest: 0.001,
            })));
        }
        scene.add(fundo);

        // Nuvens distantes, bem discretas, para dar profundidade
        for (const d of [{ rgb: '40,70,150', pos: [-70, 30, -160], esc: 120, op: 0.05 },
                         { rgb: '80,40,110', pos: [80, -40, -190], esc: 140, op: 0.04 }]) {
            const s = _sprite(_texNebulosa(d.rgb), 0xffffff, d.op, d.esc, false);
            s.position.set(...d.pos);
            scene.add(s);
        }
    }

    function init(el) {
        container = el;
        try {
            renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
        } catch (e) {
            console.warn('[Orion] WebGL indisponível — sem constelação.', e);
            return;
        }
        scene = new THREE.Scene();
        camera = new THREE.PerspectiveCamera(38, 1, 0.1, 900);
        camera.position.z = 140;
        renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
        container.appendChild(renderer.domElement);
        _construirFundo();
        _construirFigura();
        ok = true;
        resize();
        if (window.ResizeObserver) new ResizeObserver(resize).observe(container);
        else window.addEventListener('resize', resize);
        requestAnimationFrame(_quadro);
    }

    function resize() {
        if (!ok) return;
        const w = Math.max(1, container.clientWidth), h = Math.max(1, container.clientHeight);
        renderer.setSize(w, h);
        camera.aspect = w / h;
        // Janela estreita: afasta a câmera para a figura (≈45 de largura) caber
        camera.position.z = 140 * Math.max(1, 0.78 / camera.aspect);
        camera.updateProjectionMatrix();
    }

    function _pontoTrajeto(s) {
        s = ((s % trajetoLen) + trajetoLen) % trajetoLen;
        for (let i = 0; i < trajeto.length; i++) {
            const seg = trajeto[i];
            if (s <= seg.len) return { p: seg.a.clone().lerp(seg.b, s / seg.len), i };
            s -= seg.len;
        }
        return { p: trajeto[0].a.clone(), i: 0 };
    }

    let _focado = true, _pulo = 0;
    window.addEventListener('blur', () => { _focado = false; });
    window.addEventListener('focus', () => { _focado = true; });
    document.addEventListener('visibilitychange', () => { _focado = !document.hidden; });

    function _quadro(agora) {
        requestAnimationFrame(_quadro);
        // Sem foco: 1 de cada 4 quadros, para não gastar GPU em segundo plano
        if (!_focado && (_pulo = (_pulo + 1) % 4) !== 0) return;

        const dt = Math.min(0.25, (agora - ultimo) / 1000);   // teto alto: revelação termina mesmo com quadros pulados
        ultimo = agora;
        const t = agora / 1000;
        audio += (alvoAudio - audio) * 0.09;
        if (revelacao >= 0) revelacao += dt;
        if (flash < 1) flash = Math.min(1, flash + dt * 1.1);

        if (!reduzido) {
            root.rotation.y = Math.sin(t * 0.045) * 0.30;
            root.rotation.x = Math.sin(t * 0.031) * 0.05;
            fundo.rotation.y += dt * 0.004;
        } else {
            root.rotation.set(0, 0, 0);
        }

        // Traço do "processando"
        const tracoAlvo = estado === 'processing' ? 1 : 0;
        tracoOp += (tracoAlvo - tracoOp) * 0.08;
        let cabeca = null;
        segAtual = -1;
        if (tracoOp > 0.01) {
            if (!reduzido) tracoS += dt * VEL_TRACO;
            const h = _pontoTrajeto(tracoS);
            cabeca = h.p; segAtual = trajeto[h.i].linha;
            for (const s of cometa.children) {
                const k = s.userData.k;
                s.position.copy(k === 0 ? h.p : _pontoTrajeto(tracoS - k * 0.9).p);
                s.material.opacity = tracoOp * (k === 0 ? 0.95 : 0.5 * (1 - k / 7));
            }
        } else {
            for (const s of cometa.children) s.material.opacity = 0;
        }

        const tint = TINT[estado];
        for (const e of estrelas.values()) {
            const r = revelacao < 0 ? 0 : Math.min(1, Math.max(0, (revelacao - e.ordem * 0.07) / 0.5));
            const tw = reduzido ? 1 : 0.88 + 0.12 * Math.sin(t * e.ritmo + e.fase);
            let boost = 0;
            if (estado === 'listening' && e.cinturao) boost = 0.35 + audio * 1.4;
            else if (estado === 'speaking') boost = audio * 0.9;
            if (cabeca) boost = Math.max(boost, Math.max(0, 1 - cabeca.distanceTo(e.pos) / 7) * 0.9 * tracoOp);

            e.nucleo.material.opacity = r * e.op * tw;
            e.nucleo.scale.setScalar(e.tam * (1 + boost * 0.25));
            e.halo.material.opacity = r * (0.16 + boost * 0.24) * tw;
            e.halo.scale.setScalar(e.tam * 3.2 * (1 + boost * 0.5));
            _tmp.copy(e.cor);
            if (tint) _tmp.lerp(tint, 0.55);
            e.halo.material.color.lerp(_tmp, 0.06);
            if (e.flare) {
                e.flare.material.opacity = r * 0.42 * tw;
                e.flare.material.rotation = reduzido ? 0 : Math.sin(t * 0.1 + e.fase) * 0.06;
            }
            if (e.grupo.userData.neb) e.grupo.userData.neb.material.opacity = r * 0.3;
        }

        for (const l of linhas) {
            const p = revelacao < 0 ? 0 : Math.min(1, Math.max(0, (revelacao - 1.0 - l.ordem * 0.05) / 0.45));
            if (!l.pronta) {
                const fim = l.pa.clone().lerp(l.pb, p);
                const arr = l.geo.attributes.position.array;
                arr[3] = fim.x; arr[4] = fim.y; arr[5] = fim.z;
                l.geo.attributes.position.needsUpdate = true;
                if (p >= 1) l.pronta = true;
            }
            let boost = 0;
            if (l.ordem === segAtual) boost = 1.3 * tracoOp;
            else if (estado === 'speaking') boost = audio * 0.8;
            else if (estado === 'listening' && l.cinturao) boost = 0.4 + audio;
            l.mat.opacity = p > 0 ? Math.min(1, l.base * (1 + boost)) : 0;
            _tmp.copy(COR_LINHA);
            if (tint) _tmp.lerp(tint, 0.6);
            l.mat.color.lerp(_tmp, 0.06);
        }

        const rev = revelacao < 0 ? 0 : Math.min(1, revelacao / 1.2);
        const fl = flash < 1 ? (1 - flash) ** 2 : 0;
        nucleo.scale.setScalar(18 + audio * 16 + fl * 30 + (reduzido ? 0 : Math.sin(t * 1.3) * 0.8));
        nucleo.material.opacity = rev * (0.07 + audio * 0.18) + fl * 0.45;
        _tmp.set(tint ? COR[estado] : COR.idle);
        nucleo.material.color.lerp(_tmp, 0.05);

        renderer.render(scene, camera);
    }

    return {
        init, resize,
        setEstado(s) { estado = s; },
        setAudio(v) { alvoAudio = Math.max(0, Math.min(1, +v || 0)); },
        getAudio() { return audio; },
        revelar() { if (revelacao < 0) revelacao = reduzido ? 99 : 0; flash = 0; },
        setReduzido(v) { reduzido = !!v; if (v && revelacao >= 0) revelacao = 99; },
    };
})();

/* ==========================================================================
   ESTADO
   ========================================================================== */
let estadoAtual = null;

function definirEstado(s) {
    if (!ROTULO[s] || estadoAtual === s) return;
    estadoAtual = s;
    Ceu.setEstado(s);
    const cor = COR[s];
    document.body.dataset.estado = s;
    const dot = document.getElementById('state-dot');
    if (dot) { dot.style.background = cor; dot.style.boxShadow = `0 0 8px ${cor}`; }
    const lbl = document.getElementById('state-indicator');
    if (lbl) lbl.textContent = ROTULO[s];
    const sbDot = document.getElementById('dot-state');
    if (sbDot) { sbDot.style.background = cor; sbDot.style.boxShadow = `0 0 5px ${cor}`; }
    const chatEstado = document.getElementById('chat-state');
    if (chatEstado) chatEstado.textContent = s === 'idle' ? '' : ROTULO[s].toLowerCase();
    if (s === 'processing') Ceu.setAudio(0.3);
    if (s === 'idle') setTimeout(() => { if (estadoAtual === 'idle') Ceu.setAudio(0); }, 600);
}

/* ==========================================================================
   CHAT
   ========================================================================== */
const Chat = (() => {
    const MAX_ITENS = 80;
    const LIMIAR_DEMORA_MS = 8000;
    let aberto = false;
    let atual = null;              // {el, texto, corpo, tier, final}
    let timerDemora = null;

    const lista = () => document.getElementById('chat-messages');

    function _podar() {
        const c = lista();
        while (c && c.children.length >= MAX_ITENS) c.removeChild(c.firstChild);
    }

    /* Markdown leve: blocos de código, código inline, negrito, itálico,
       listas, títulos e imagens. Escapa tudo antes (aspas inclusive). */
    function markdown(bruto) {
        const partes = String(bruto).split(/```([\s\S]*?)```/g);
        let html = '';
        for (let i = 0; i < partes.length; i++) {
            if (i % 2 === 1) {
                const corpo = partes[i].replace(/^[\w+-]*\n/, '');
                html += `<pre><code>${_escapar(corpo.replace(/\n$/, ''))}</code></pre>`;
                continue;
            }
            let seg = _escapar(partes[i]);
            seg = seg.replace(/!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/g,
                '<img src="$2" alt="$1" class="msg-img" loading="lazy">');
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

    function _botao(classe, rotulo, svg, aoClicar) {
        const b = document.createElement('button');
        b.className = 'msg-act ' + classe;
        b.setAttribute('aria-label', rotulo);
        b.dataset.tip = rotulo;
        b.innerHTML = svg;
        b.addEventListener('click', e => { e.stopPropagation(); aoClicar(b); });
        return b;
    }
    const SVG_COPIAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/></svg>';
    const SVG_OUVIR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4V5Z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/></svg>';

    function _acoes(textoFn, comVoz) {
        const box = document.createElement('div');
        box.className = 'msg-actions';
        box.appendChild(_botao('', 'Copiar', SVG_COPIAR, b => {
            navigator.clipboard?.writeText(textoFn()).then(() => {
                b.classList.add('ok');
                setTimeout(() => b.classList.remove('ok'), 900);
            }).catch(() => {});
        }));
        if (comVoz) box.appendChild(_botao('', 'Ouvir', SVG_OUVIR, b => {
            const txt = textoFn().trim();
            if (!txt) return;
            b.classList.add('ok');
            setTimeout(() => b.classList.remove('ok'), 1200);
            fetch(CEREBRO + '/tts/falar', {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ texto: txt }),
            }).catch(() => {});
        }));
        return box;
    }

    function _mostrar(el) { requestAnimationFrame(() => el.classList.add('visible')); }

    function usuario(texto, { animar = true } = {}) {
        _podar();
        const el = document.createElement('div');
        el.className = 'msg msg-user';
        const t = document.createElement('div');
        t.className = 'msg-text';
        t.textContent = texto;
        el.appendChild(t);
        el.appendChild(_acoes(() => texto, false));
        lista()?.appendChild(el);
        if (animar) _mostrar(el); else el.classList.add('visible');
        if (!aberto) badge(true);
        rolar(true);
    }

    function _novaOrion({ digitando = false, quando = new Date(), animar = true } = {}) {
        _podar();
        const el = document.createElement('div');
        el.className = 'msg msg-orion';
        const cab = document.createElement('div');
        cab.className = 'msg-head';
        cab.innerHTML = '<span class="belt-mark" aria-hidden="true"><i></i><i></i><i></i></span>' +
                        '<span class="msg-who">Orion</span>';
        const hora = document.createElement('span');
        hora.className = 'msg-time';
        hora.textContent = typeof quando === 'string' ? quando : _hora(quando);
        const tier = document.createElement('span');
        tier.className = 'msg-tier';
        cab.appendChild(hora); cab.appendChild(tier);
        const corpo = document.createElement('div');
        corpo.className = 'msg-text';
        if (digitando) corpo.innerHTML = '<span class="dots"><i></i><i></i><i></i></span>';
        const demora = document.createElement('div');
        demora.className = 'msg-slow';
        demora.textContent = 'Ainda processando…';
        el.appendChild(cab); el.appendChild(corpo); el.appendChild(demora);
        lista()?.appendChild(el);
        if (animar) _mostrar(el); else el.classList.add('visible');
        const obj = { el, corpo, tier, demora, texto: '', final: false, acoes: false };
        return obj;
    }

    function _armarDemora() {
        _desarmarDemora();
        timerDemora = setTimeout(() => atual?.demora.classList.add('show'), LIMIAR_DEMORA_MS);
    }
    function _desarmarDemora() {
        if (timerDemora) { clearTimeout(timerDemora); timerDemora = null; }
        atual?.demora.classList.remove('show');
    }

    /* Início de uma resposta (estado "processando") */
    function digitando() {
        if (atual && !atual.final && !atual.texto) return;   // já tem um indicador vazio
        atual = _novaOrion({ digitando: true });
        _armarDemora();
        Som.mensagem();
        if (!aberto) badge(true);
        rolar(true);
    }

    /* Trecho de resposta em streaming (hub ou voz ao vivo) */
    function trecho(pedaco) {
        _desarmarDemora();
        if (!atual || atual.final) atual = _novaOrion();
        atual.texto += pedaco;
        atual.corpo.innerHTML = markdown(atual.texto);
        if (!atual.acoes) {
            const ref = atual;
            ref.el.appendChild(_acoes(() => ref.texto, true));
            ref.acoes = true;
        }
        rolar();
    }

    function fim() {
        _desarmarDemora();
        if (!atual) return;
        if (!atual.texto) atual.el.remove();      // indicador sem resposta nenhuma
        atual.final = true;
        atual = null;
    }

    function tier(nome) {
        if (atual) atual.tier.textContent = nome;
    }

    /* Aviso do sistema (erro, confirmação) — não é fala do Orion */
    function nota(texto, tipo = '') {
        _podar();
        const el = document.createElement('div');
        el.className = 'msg-note' + (tipo ? ' ' + tipo : '');
        el.textContent = texto;
        lista()?.appendChild(el);
        rolar(true);
    }

    function limparDOM() {
        const c = lista();
        if (c) c.innerHTML = '';
        atual = null;
        _desarmarDemora();
    }

    function renderHistorico(msgs) {
        for (const m of (msgs || []).slice(-60)) {
            if (m.role === 'user') { usuario(m.content, { animar: false }); continue; }
            const quando = m.timestamp
                ? new Date(m.timestamp).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
                : '';
            const o = _novaOrion({ quando, animar: false });
            o.texto = m.content || '';
            o.corpo.innerHTML = markdown(o.texto);
            o.el.appendChild(_acoes(() => o.texto, true));
            o.final = true;
        }
        badge(false);
        rolar(true);
    }

    const PERTO_DO_FIM = 56;
    function rolar(forcar) {
        const c = lista();
        if (!c) return;
        const perto = c.scrollHeight - c.scrollTop - c.clientHeight < PERTO_DO_FIM;
        if (forcar || perto) c.scrollTop = c.scrollHeight;
        _botaoRolar();
    }
    function _botaoRolar() {
        const c = lista(), b = document.getElementById('chat-scroll-btn');
        if (!c || !b) return;
        const perto = c.scrollHeight - c.scrollTop - c.clientHeight < PERTO_DO_FIM;
        b.classList.toggle('show', !perto && c.scrollHeight > c.clientHeight);
    }
    function badge(on) {
        document.getElementById('chat-badge')?.classList.toggle('show', on);
    }

    lista()?.addEventListener('scroll', _botaoRolar);

    return {
        usuario, digitando, trecho, fim, tier, nota, limparDOM, renderHistorico, rolar, badge, markdown,
        get aberto() { return aberto; },
        set aberto(v) { aberto = !!v; if (aberto) badge(false); },
    };
})();

/* ==========================================================================
   HUB WS :8765 — estado, falas e trechos de resposta
   ========================================================================== */
function _conectarHub() {
    let ws, espera = 2000;
    const conectar = () => {
        try {
            ws = new WebSocket(HUB_WS);
            ws.onopen = () => { espera = 2000; _pontoStatus('ws', true); esconderToast(); };
            ws.onmessage = ({ data }) => {
                let m;
                try { m = JSON.parse(data); } catch (_) { return; }
                if (m.state === 'processing') Chat.digitando();
                if (m.state != null) definirEstado(m.state);
                if (m.intensity != null) Ceu.setAudio(m.intensity);
                if (m.user_text) Chat.usuario(m.user_text);
                if (m.tier) {
                    const el = document.getElementById('p-fonte-label');
                    if (el) el.textContent = m.tier;
                    Chat.tier(m.tier);
                }
                if (m.ai_chunk) Chat.trecho(m.ai_chunk);
                if (m.state === 'idle') Chat.fim();
            };
            ws.onclose = () => {
                _pontoStatus('ws', false);
                toast('Hub desconectado — reconectando…');
                setTimeout(conectar, espera);
                espera = Math.min(espera * 1.5, 15000);
            };
            ws.onerror = () => ws.close();
        } catch (_) { setTimeout(conectar, espera); }
    };
    conectar();
}

/* ==========================================================================
   ENVIO — composers da home e do chat
   ========================================================================== */
const _historicoEnvio = { itens: [], idx: 0, rascunho: '', MAX: 50 };

function enviar(texto) {
    const v = (texto || '').trim();
    if (!v) return;
    const h = _historicoEnvio;
    if (h.itens[h.itens.length - 1] !== v) {
        h.itens.push(v);
        if (h.itens.length > h.MAX) h.itens.shift();
    }
    h.idx = h.itens.length;
    h.rascunho = '';
    Som.envio();
    const fonte = document.getElementById('p-fonte-label');
    if (fonte) fonte.textContent = '…';
    if (window.pywebview?.api) {
        const modelo = document.getElementById('sel-modelo')?.value || 'auto';
        window.pywebview.api.process_command(v, modelo);
    } else {
        Chat.usuario(v);
        Chat.nota('Sem a ponte do pywebview: abra pelo orion_app.py para conversar.', 'warn');
    }
}

function _ligarComposer(inp, { abreChat = false } = {}) {
    if (!inp) return;
    inp.addEventListener('keydown', e => {
        const h = _historicoEnvio;
        // ↑/↓ percorrem o que já foi enviado (só quando o campo está vazio ou já navegando)
        if (e.key === 'ArrowUp' && h.itens.length && (!inp.value || h.idx < h.itens.length)) {
            e.preventDefault();
            if (h.idx === h.itens.length) h.rascunho = inp.value;
            if (h.idx > 0) h.idx--;
            inp.value = h.itens[h.idx];
            return;
        }
        if (e.key === 'ArrowDown' && h.idx < h.itens.length) {
            e.preventDefault();
            h.idx++;
            inp.value = h.idx === h.itens.length ? h.rascunho : h.itens[h.idx];
            return;
        }
        if (e.key !== 'Enter') return;
        const v = inp.value.trim();
        if (!v) return;
        inp.value = '';
        if (abreChat) OrionUI.showView('chat');
        enviar(v);
    });
    // Colar imagem (ex.: print) envia como anexo
    inp.addEventListener('paste', e => {
        for (const item of e.clipboardData?.items || []) {
            if (item.type?.startsWith('image/')) {
                e.preventDefault();
                const f = item.getAsFile();
                if (f) { if (abreChat) OrionUI.showView('chat'); handleFileAttach([f]); }
                return;
            }
        }
    });
}

function _configurarEntrada() {
    _ligarComposer(document.getElementById('orion-input'));
    _ligarComposer(document.getElementById('home-input'), { abreChat: true });
    document.getElementById('btn-send')?.addEventListener('click', () => {
        const inp = document.getElementById('orion-input');
        if (!inp?.value.trim()) return;
        const v = inp.value; inp.value = '';
        enviar(v);
        inp.focus();
    });

    document.addEventListener('contextmenu', e => {
        if (!e.target.closest('input, .msg-text')) e.preventDefault();
    });
    document.addEventListener('keydown', e => {
        if (e.key === 'Escape') {
            if (document.getElementById('grafo-overlay')?.classList.contains('open')) { fecharGrafo(); return; }
            OrionUI.escapeView();
            return;
        }
        const digitandoAgora = ['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement?.tagName);
        const grafoAberto = document.getElementById('grafo-overlay')?.classList.contains('open');
        if (e.key === '/' && !digitandoAgora && !grafoAberto) {
            e.preventDefault();
            if (OrionUI.currentView() === 'home') document.getElementById('home-input')?.focus();
            else OrionUI.showView('chat');
        }
    });
}

/* ==========================================================================
   ANEXOS — clipe, colar ou arrastar
   ========================================================================== */
const _TIPOS_VIDEO = ['video/mp4', 'video/quicktime', 'video/webm'];

function _configurarArrastar() {
    let profundidade = 0;
    const alvo = () => document.getElementById('chat-view');
    document.addEventListener('dragover', e => e.preventDefault());
    document.addEventListener('dragenter', e => {
        e.preventDefault();
        if (!e.dataTransfer?.types?.includes('Files')) return;
        profundidade++;
        document.body.classList.add('drag-over');
        alvo()?.classList.add('drag-over');
    });
    document.addEventListener('dragleave', e => {
        e.preventDefault();
        if (--profundidade <= 0) {
            profundidade = 0;
            document.body.classList.remove('drag-over');
            alvo()?.classList.remove('drag-over');
        }
    });
    document.addEventListener('drop', e => {
        e.preventDefault();
        profundidade = 0;
        document.body.classList.remove('drag-over');
        alvo()?.classList.remove('drag-over');
        const f = [...(e.dataTransfer?.files || [])].find(x => /^(image|audio)\//.test(x.type) || _TIPOS_VIDEO.includes(x.type));
        if (!f) { toast('Tipo não suportado — use imagem, áudio ou vídeo.', 3200); return; }
        OrionUI.showView('chat');
        handleFileAttach([f]);
    });
}

window.handleFileAttach = async (files) => {
    const f = files?.[0];
    if (!f) return;
    const input = document.getElementById('file-attach');
    const btn = document.getElementById('btn-attach');
    btn?.classList.add('busy');
    toast(`Enviando ${f.name}…`);
    try {
        const form = new FormData();
        form.append('file', f);
        const res = await fetch(CEREBRO + '/upload', { method: 'POST', body: form });
        const r = await res.json();
        if (!r.ok) throw new Error(r.erro || 'falha no upload');
        const pedido = f.type.startsWith('image/')
            ? 'O que tem nessa imagem que anexei (use analisar_imagem)'
            : 'Transcreva esse áudio/vídeo que anexei (use transcrever_audio)';
        esconderToast();
        enviar(`${pedido}: ${r.path}`);
    } catch (e) {
        toast('Falha ao enviar arquivo: ' + e.message, 4000);
    } finally {
        if (input) input.value = '';
        btn?.classList.remove('busy');
    }
};

/* ==========================================================================
   VOZ AO VIVO — Gemini Live via /ws/voice (independe do hub e do mic_engine)
   ========================================================================== */
const Voz = (() => {
    let ws = null, stream = null, ctx = null, fonte = null, proc = null;
    let ctxPlay = null, proximo = 0, ativa = false;

    const botoes = () => document.querySelectorAll('.voice-live-btn');
    const marcar = on => botoes().forEach(b => b.classList.toggle('active', on));

    function _ctxPlay() {
        if (!ctxPlay) {
            try { ctxPlay = new (window.AudioContext || window.webkitAudioContext)(); } catch (_) { ctxPlay = null; }
        }
        return ctxPlay;
    }

    function _tocar(buf) {
        const c = _ctxPlay();
        if (!c || !buf.byteLength) return;
        const i16 = new Int16Array(buf), f32 = new Float32Array(i16.length);
        let pico = 0;
        for (let i = 0; i < i16.length; i++) { f32[i] = i16[i] / 32768; pico = Math.max(pico, Math.abs(f32[i])); }
        const ab = c.createBuffer(1, f32.length, 24000);
        ab.getChannelData(0).set(f32);
        const src = c.createBufferSource();
        src.buffer = ab;
        src.connect(c.destination);
        const ini = Math.max(c.currentTime, proximo);
        src.start(ini);
        proximo = ini + ab.duration;
        definirEstado('speaking');
        Ceu.setAudio(Math.min(1, pico * 1.6));
    }

    function _cmd(cmd) { if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ cmd })); }

    async function iniciar() {
        if (ativa) return;
        ativa = true;
        marcar(true);
        try {
            stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        } catch (e) {
            Chat.nota('Sem acesso ao microfone: ' + e.message, 'warn');
            ativa = false; marcar(false);
            return;
        }
        ctx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 16000 });
        fonte = ctx.createMediaStreamSource(stream);
        proc = ctx.createScriptProcessor(4096, 1, 1);
        const mudo = ctx.createGain();
        mudo.gain.value = 0;
        fonte.connect(proc); proc.connect(mudo); mudo.connect(ctx.destination);
        proc.onaudioprocess = e => {
            if (ws?.readyState !== WebSocket.OPEN) return;
            const f32 = e.inputBuffer.getChannelData(0), i16 = new Int16Array(f32.length);
            let pico = 0;
            for (let i = 0; i < f32.length; i++) {
                const s = Math.max(-1, Math.min(1, f32[i]));
                i16[i] = s < 0 ? s * 32768 : s * 32767;
                pico = Math.max(pico, Math.abs(s));
            }
            if (estadoAtual === 'listening') Ceu.setAudio(Math.min(1, pico * 2));
            ws.send(i16.buffer);
        };

        ws = new WebSocket(VOICE_WS);
        ws.binaryType = 'arraybuffer';
        ws.onopen = () => { _cmd('start'); definirEstado('listening'); };
        ws.onmessage = ({ data }) => {
            if (data instanceof ArrayBuffer) { _tocar(data); return; }
            let m;
            try { m = JSON.parse(data); } catch (_) { return; }
            if (m.type === 'text' && m.text) Chat.trecho(m.text);
            if (m.type === 'done') { Chat.fim(); if (ativa) definirEstado('listening'); }
            if (m.type === 'error') { Chat.nota('Voz ao vivo: ' + m.msg, 'warn'); parar(); }
        };
        ws.onerror = () => { Chat.nota('Voz ao vivo: falha na conexão.', 'warn'); parar(); };
        ws.onclose = () => { if (ativa) parar(); };
    }

    function parar() {
        if (!ativa) return;
        ativa = false;
        marcar(false);
        _cmd('stop');
        try { ws?.close(); } catch (_) {}
        ws = null;
        stream?.getTracks().forEach(t => t.stop());
        stream = null;
        try { proc?.disconnect(); fonte?.disconnect(); } catch (_) {}
        try { ctx?.close(); } catch (_) {}
        ctx = fonte = proc = null;
        proximo = 0;
        Chat.fim();
        definirEstado('idle');
    }

    function configurar() {
        botoes().forEach(b => b.addEventListener('click', () => (ativa ? parar() : iniciar())));
    }

    return { configurar, iniciar, parar };
})();

/* ==========================================================================
   STATUS — pontos da sidebar, cérebro, modelo, sidebar recolhida
   ========================================================================== */
function _pontoStatus(id, online) {
    const dot = document.getElementById(`dot-${id}`);
    if (!dot) return;
    const cor = online ? COR.ok : COR.off;
    dot.style.background = cor;
    dot.style.boxShadow = `0 0 5px ${cor}`;
}

function _configurarStatus() {
    const pingar = async () => {
        try { await _fetchTimeout(CEREBRO + '/', 2000); _pontoStatus('cerebro', true); }
        catch (_) { _pontoStatus('cerebro', false); }
    };
    pingar();
    setInterval(pingar, 8000);

    const sel = document.getElementById('sel-modelo');
    if (sel) {
        sel.value = Prefs.get('modelo', 'auto');
        sel.addEventListener('change', () => Prefs.set('modelo', sel.value));
    }
}

function togglePanel() {
    const recolhida = document.body.classList.toggle('sb-collapsed');
    Prefs.set('sb_recolhida', recolhida ? '1' : '0');
    const b = document.getElementById('panel-toggle');
    if (b) b.dataset.tip = recolhida ? 'Expandir sidebar' : 'Recolher sidebar';
}

/* ==========================================================================
   TOAST
   ========================================================================== */
let _timerToast = null;
function toast(texto, ms = 0) {
    const el = document.getElementById('toast');
    if (!el) return;
    el.textContent = texto;
    el.classList.add('show');
    if (_timerToast) clearTimeout(_timerToast);
    _timerToast = ms ? setTimeout(() => el.classList.remove('show'), ms) : null;
}
function esconderToast() {
    const el = document.getElementById('toast');
    if (!el) return;
    if (_timerToast) clearTimeout(_timerToast);
    _timerToast = setTimeout(() => el.classList.remove('show'), 400);
}

/* ==========================================================================
   AÇÕES DO CHAT
   ========================================================================== */
async function limparHistorico() {
    if (!confirm('Limpar o histórico desta sessão em memória? A memória de longo prazo não é afetada.')) return;
    try {
        await fetch(CEREBRO + '/historico', { method: 'DELETE' });
        Chat.limparDOM();
        Chat.nota('Histórico em memória limpo.');
    } catch (e) {
        Chat.nota('Falha ao limpar histórico: ' + e.message, 'warn');
    }
}

async function exportarConversa() {
    try {
        const d = await (await fetch(CEREBRO + '/exportar')).json();
        if (!d.markdown || d.total_msgs === 0) { toast('Nada para exportar ainda.', 2600); return; }
        const url = URL.createObjectURL(new Blob([d.markdown], { type: 'text/markdown;charset=utf-8' }));
        const a = document.createElement('a');
        a.href = url;
        a.download = `orion-conversa-${new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-')}.md`;
        document.body.appendChild(a); a.click(); a.remove();
        URL.revokeObjectURL(url);
        toast(`Conversa exportada (${d.total_msgs} mensagens).`, 2600);
    } catch (e) {
        toast('Falha ao exportar: ' + e.message, 3500);
    }
}

/* ==========================================================================
   CONFIGURAÇÕES — atividade, cascata, serviços
   ========================================================================== */
function _linhaSpark(el, valores, max) {
    if (!el) return;
    if (!valores || valores.length < 2) { el.setAttribute('points', ''); return; }
    const w = 200, h = 32;
    const hi = Math.max(...valores, 1), lo = Math.min(...valores), span = Math.max(hi - lo, 1);
    const passo = w / ((max || valores.length) - 1);
    el.setAttribute('points', valores.map((v, i) =>
        `${(i * passo).toFixed(1)},${(h - 2 - ((v - lo) / span) * (h - 4)).toFixed(1)}`).join(' '));
}

function _configurarMetricas() {
    const $ = id => document.getElementById(id);
    const latencias = [];
    const MAX_LAT = 24;
    const pct = v => v != null ? `${Math.round(v)}%` : '—';

    const metricas = async () => {
        try {
            const m = await (await _fetchTimeout(CEREBRO + '/metrics', 2000)).json();
            $('m-cpu').textContent = pct(m.cpu_pct);
            $('m-ram').textContent = pct(m.ram_pct);
            $('m-gpu').textContent = pct(m.gpu_pct);
            $('m-vram').textContent = pct(m.vram_pct);
            if (m.latencia_ms != null) {
                $('m-latencia').textContent = `${m.latencia_ms} ms`;
                latencias.push(m.latencia_ms);
                if (latencias.length > MAX_LAT) latencias.shift();
                _linhaSpark($('m-spark-line'), latencias, MAX_LAT);
            }
        } catch (_) {
            ['m-latencia', 'm-cpu', 'm-ram', 'm-gpu', 'm-vram'].forEach(id => { $(id).textContent = '—'; });
        }
    };

    const cascata = async () => {
        try {
            const s = await (await _fetchTimeout(CEREBRO + '/stats', 2500)).json();
            $('t-total').textContent = s.total_chats ?? '0';
            const box = $('t-tiers');
            if (!box || !s.tiers) return;
            box.innerHTML = '';
            for (const nome of ['Groq', 'Gemini', 'Claude', 'Local']) {
                const t = s.tiers[nome];
                if (!t) continue;
                const p = s.distribuicao_pct?.[nome] ?? 0;
                const lat = t.latencia_media_ms != null ? `${t.latencia_media_ms} ms` : '—';
                const row = document.createElement('div');
                row.className = 'tier';
                row.dataset.tip = `${t.usos} usos · ${t.falhas} falhas · ${lat}`;
                row.innerHTML = `<div class="tier-top"><span></span><span class="mono"></span></div>` +
                                `<div class="tier-bar"><span></span></div>`;
                row.querySelector('.tier-top span').textContent = nome;
                row.querySelector('.tier-top .mono').textContent = `${p}%`;
                row.querySelector('.tier-bar span').style.width = `${p}%`;
                box.appendChild(row);
            }
        } catch (_) {}
    };

    const servicos = async () => {
        const marcar = (svc, online, txt) => {
            const dot = $(`dot-${svc}`);
            if (dot) {
                const cor = online ? COR.ok : COR.off;
                dot.style.background = cor;
                dot.style.boxShadow = `0 0 5px ${cor}`;
            }
            const el = $(`svc-${svc}`);
            if (el) el.textContent = txt;
        };
        try {
            const h = await (await _fetchTimeout(CEREBRO + '/health', 3000)).json();
            for (const svc of ['qdrant', 'surreal', 'ollama']) {
                const s = h[svc] || {};
                marcar(svc, s.ok, s.ok ? `${s.latencia_ms ?? '—'} ms` : 'fora do ar');
            }
        } catch (_) {
            for (const svc of ['qdrant', 'surreal', 'ollama']) marcar(svc, false, 'sem resposta');
        }
    };

    const historico = async () => {
        try {
            const { snapshots } = await (await _fetchTimeout(CEREBRO + '/stats/historico?limite=48', 3000)).json();
            _linhaSpark($('t-hist-spark-line'), (snapshots || []).map(s => s.total_chats || 0));
        } catch (_) {}
    };

    metricas(); setInterval(metricas, 4000);
    const lento = () => { cascata(); servicos(); };
    lento(); setInterval(lento, 10000);
    historico(); setInterval(historico, 60000);
}

/* ==========================================================================
   GRAFO DE MEMÓRIA — /grafo/completo em 3d-force-graph
   ========================================================================== */
let _grafo = null, _grafoDados = null;
const NUCLEO_ID = 'orion:core';
const _ehUsuario = ator => /^ant[oô]nio$/i.test(String(ator || '').trim());

function _grafoDemo() {
    const temas = ['memória', 'sqlite', 'telegram', 'tailscale', 'voz', 'python', 'fase 0',
                   'mcp', 'backup', 'notebook', 'obsidian', 'agenda'];
    const frases = ['Exportar as tabelas pessoais antes da venda do PC', 'Memória nova em SQLite com busca híbrida',
                    'Bot do Telegram como canal principal no celular', 'Acesso de fora pela rede do Tailscale',
                    'Voz masculina com palavra de ativação', 'Backup diário do arquivo de memória'];
    const nodes = temas.map(t => ({ id: `topico:${t}`, tipo: 'topico', label: t }));
    const links = [];
    for (let i = 0; i < 48; i++) {
        const id = `evento:demo${i}`;
        nodes.push({ id, tipo: 'evento', label: frases[i % frases.length], ator: i % 2 ? 'Antônio' : 'Orion' });
        for (let k = 0; k < 2; k++) links.push({ source: id, target: `topico:${temas[(i * 3 + k * 5) % temas.length]}` });
    }
    return { nodes, links };
}

function abrirGrafo() {
    document.getElementById('grafo-overlay')?.classList.add('open');
    _carregarGrafo();
}

function fecharGrafo() {
    document.getElementById('grafo-overlay')?.classList.remove('open');
    document.getElementById('grafo-detail')?.classList.remove('show');
    window.OrionUI?.syncNav?.();
}

async function _carregarGrafo() {
    const box = document.getElementById('grafo-container');
    if (!box) return;
    let carregando = document.getElementById('grafo-loading');
    if (!carregando) {
        carregando = document.createElement('div');
        carregando.id = 'grafo-loading';
        carregando.textContent = 'Carregando memória…';
        box.appendChild(carregando);
    }
    carregando.style.display = 'flex';
    let demo = false;
    try {
        const res = await _fetchTimeout(CEREBRO + '/grafo/completo?limite=500', 6000);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        _grafoDados = await res.json();
        if (!_grafoDados.nodes?.length) throw new Error('vazio');
    } catch (_) {
        demo = true;
        _grafoDados = _grafoDemo();
    }
    carregando.style.display = 'none';
    const stats = document.getElementById('grafo-stats');
    if (stats) stats.textContent = `${_grafoDados.nodes?.length ?? 0} nós · ${_grafoDados.links?.length ?? 0} ligações${demo ? ' · demonstração' : ''}`;
    _desenharGrafo(_grafoDados);
}

function _desenharGrafo(dados) {
    const box = document.getElementById('grafo-container');
    if (!box || typeof ForceGraph3D === 'undefined') return;
    if (_grafo) {
        _grafo._destructor?.();
        _grafo = null;
        box.innerHTML = '';
    }
    const W = box.clientWidth || 900, H = box.clientHeight || 580;
    const RAIO = 70;
    const texNo = (() => {
        const sz = 64, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const gr = ctx.createRadialGradient(sz / 2, sz / 2, 0, sz / 2, sz / 2, sz / 2);
        gr.addColorStop(0, 'rgba(255,255,255,1)'); gr.addColorStop(0.2, 'rgba(255,255,255,0.8)');
        gr.addColorStop(0.5, 'rgba(255,255,255,0.16)'); gr.addColorStop(1, 'rgba(255,255,255,0)');
        ctx.fillStyle = gr; ctx.fillRect(0, 0, sz, sz);
        return new THREE.CanvasTexture(cv);
    })();

    const grau = {};
    for (const l of dados.links || []) {
        const s = l.source?.id ?? l.source, t = l.target?.id ?? l.target;
        grau[s] = (grau[s] || 0) + 1; grau[t] = (grau[t] || 0) + 1;
    }
    const nodes = [...(dados.nodes || [])];
    // Link apontando para nó inexistente derruba o 3d-force-graph: filtra aqui
    const validos = new Set(nodes.map(n => n.id));
    const links = (dados.links || []).filter(l => validos.has(l.source) && validos.has(l.target));
    nodes.push({ id: NUCLEO_ID, tipo: 'nucleo', label: 'Orion', fx: 0, fy: 0, fz: 0 });
    nodes.filter(n => n.tipo === 'topico').forEach(t => links.push({ source: NUCLEO_ID, target: t.id, rel: 'nucleo' }));
    nodes.forEach(n => { n._grau = grau[n.id] || 1; });
    links.forEach(l => {
        if (!('_curv' in l)) {
            l._curv = l.rel === 'nucleo' ? 0.15 + Math.random() * 0.2 : 0;
            l._rot = Math.random() * PI * 2;
        }
    });

    const casca = nodes.filter(n => n.id !== NUCLEO_ID);
    casca.forEach((n, i) => {
        const phi = Math.acos(1 - 2 * (i + 0.5) / casca.length), th = PI * (1 + Math.sqrt(5)) * i;
        n.x = RAIO * Math.sin(phi) * Math.cos(th);
        n.y = RAIO * Math.sin(phi) * Math.sin(th);
        n.z = RAIO * Math.cos(phi);
    });

    const CORES = {
        topico: new THREE.Color(COR.processing),
        orion:  new THREE.Color(COR.idle),
        user:   new THREE.Color(0x9aa6bd),
    };
    const objNo = n => {
        const g = new THREE.Group();
        const spr = (tam, cor, op) => {
            const s = new THREE.Sprite(new THREE.SpriteMaterial({
                map: texNo, color: cor, blending: THREE.AdditiveBlending,
                transparent: true, opacity: op, depthWrite: false,
            }));
            s.scale.set(tam, tam, 1);
            g.add(s);
        };
        if (n.id === NUCLEO_ID) {
            spr(30, new THREE.Color(0x24366b), 0.12);
            spr(14, new THREE.Color(COR.idle), 0.32);
            spr(6, new THREE.Color(0xdfe7ff), 0.75);
            spr(2.6, new THREE.Color(0xffffff), 1);
            return g;
        }
        const topico = n.tipo === 'topico';
        const cor = topico ? CORES.topico : (_ehUsuario(n.ator) ? CORES.user : CORES.orion);
        const boost = 1 + Math.log2(Math.max(n._grau, 1)) * (topico ? 0.45 : 0.2);
        const base = (topico ? 2.4 : 1.4) * boost;
        spr(base * 3, cor, topico ? 0.05 : 0.03);
        spr(base * 1.7, cor, topico ? 0.16 : 0.1);
        spr(base, cor, topico ? 0.92 : 0.8);
        return g;
    };

    const forcaEsfera = (() => {
        let ns = [];
        function f(alpha) {
            for (const n of ns) {
                if (n.id === NUCLEO_ID) continue;
                const d = Math.hypot(n.x || 0, n.y || 0, n.z || 0) || 0.001;
                const k = (RAIO / d - 1) * 0.9 * alpha;
                n.vx = (n.vx || 0) + (n.x || 0) * k;
                n.vy = (n.vy || 0) + (n.y || 0) * k;
                n.vz = (n.vz || 0) + (n.z || 0) * k;
            }
        }
        f.initialize = x => { ns = x; };
        return f;
    })();

    const rotulo = s => (s && s.length <= 30 && !s.includes(' ')) ? s.charAt(0).toUpperCase() + s.slice(1) : (s || '');
    _grafo = ForceGraph3D({ controlType: 'orbit' })(box)
        .width(W).height(H)
        .backgroundColor('rgba(0,0,0,0)')
        .showNavInfo(false)
        .graphData({ nodes, links })
        .nodeLabel(n => _escapar(n.id === NUCLEO_ID ? 'Orion' : rotulo(n.label || n.id)))
        .nodeThreeObject(objNo)
        .nodeThreeObjectExtend(false)
        .linkColor(l => l.rel === 'nucleo' ? 'rgba(124,156,255,0.28)' : 'rgba(0,0,0,0)')
        .linkWidth(l => l.rel === 'nucleo' ? 0.4 : 0)
        .linkCurvature(l => l._curv || 0)
        .linkCurveRotation(l => l._rot || 0)
        .d3AlphaDecay(0.04)
        .d3VelocityDecay(0.35)
        .cooldownTicks(200)
        .onEngineStop(() => {
            const c = _grafo?.controls();
            if (c && !movimentoReduzido()) { c.autoRotate = true; c.autoRotateSpeed = 0.28; }
        })
        .onNodeClick(n => _detalheGrafo(n))
        .onBackgroundClick(() => document.getElementById('grafo-detail')?.classList.remove('show'));

    // Campo de estrelas ao fundo
    const pos = new Float32Array(1200 * 3);
    for (let i = 0; i < 1200; i++) {
        const ph = Math.acos(2 * Math.random() - 1), th = Math.random() * PI * 2, r = 380 + Math.random() * 200;
        pos[i * 3] = r * Math.sin(ph) * Math.cos(th);
        pos[i * 3 + 1] = r * Math.sin(ph) * Math.sin(th);
        pos[i * 3 + 2] = r * Math.cos(ph);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    _grafo.scene().add(new THREE.Points(geo, new THREE.PointsMaterial({
        color: 0xc8d4ff, size: 0.55, transparent: true, opacity: 0.28, depthWrite: false,
    })));

    _grafo.d3Force('radial-esfera', forcaEsfera);
    _grafo.d3Force('charge').strength(-10);
    _grafo.d3Force('link')?.distance(RAIO * 0.8).strength(0.03);
    _grafo.cameraPosition({ x: 0, y: 0, z: RAIO * 3.6 });
    const ctrl = _grafo.controls();
    if (ctrl) { ctrl.enableDamping = true; ctrl.dampingFactor = 0.07; }

    const busca = document.getElementById('grafo-search');
    if (busca) {
        busca.value = '';
        busca.oninput = () => {
            const q = busca.value.trim().toLowerCase();
            if (!_grafo) return;
            _grafo.graphData({
                nodes: q ? nodes.filter(n => n.id === NUCLEO_ID || (n.label || n.id).toLowerCase().includes(q)) : nodes,
                links,
            });
        };
    }
}

function _detalheGrafo(n) {
    const el = document.getElementById('grafo-detail');
    if (!el) return;
    el.innerHTML = '';
    const add = (classe, texto) => {
        const d = document.createElement('div');
        d.className = classe;
        d.textContent = texto;
        el.appendChild(d);
    };
    if (n.id === NUCLEO_ID) {
        add('gd-tipo', 'Núcleo');
        add('gd-texto', 'Orion — todos os tópicos de memória partem daqui.');
    } else {
        const topico = n.tipo === 'topico';
        add('gd-tipo', topico ? 'Tópico' : (_ehUsuario(n.ator) ? 'Fala sua' : 'Fala do Orion'));
        add('gd-texto', n.label || n.id);
        if (n.ts) add('gd-meta', new Date(n.ts).toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }));
        if (n._grau) add('gd-meta', `${n._grau} ${n._grau === 1 ? 'ligação' : 'ligações'}`);
    }
    el.classList.add('show');
}

/* ==========================================================================
   JANELA E VOZ DE RESPOSTA
   ========================================================================== */
const _api = () => window.pywebview?.api;

window.closeOrion = () => {
    document.getElementById('close-overlay')?.classList.add('show');
    setTimeout(() => { _api()?.close_app() ?? window.close(); }, 420);
};
window.maximizeOrion = () => _api()?.toggle_maximize?.();
window.minimizeOrion = () => _api()?.minimize_app?.();

/* TTS: o estado vive no backend (quem fala é o audio_manager); aqui só liga/desliga */
let _ttsMudo = Prefs.get('tts_mudo') === '1';

function ttsMudo() { return _ttsMudo; }

function _iconeMudo() {
    const b = document.getElementById('btn-mute');
    b?.classList.toggle('muted', _ttsMudo);
    if (b) b.dataset.tip = _ttsMudo ? 'Resposta por voz desligada' : 'Resposta por voz ligada';
    document.getElementById('mute-slash')?.setAttribute('style', _ttsMudo ? '' : 'display:none');
    document.getElementById('mute-waves')?.setAttribute('style', _ttsMudo ? 'display:none' : '');
}

async function _sincronizarMudo(valor) {
    try {
        await fetch(CEREBRO + '/tts/mudo', {
            method: 'POST', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ mudo: valor }),
        });
    } catch (_) { /* cérebro fora: aplica local e sincroniza na próxima vez */ }
}

window.toggleTtsMute = () => {
    _ttsMudo = !_ttsMudo;
    Prefs.set('tts_mudo', _ttsMudo ? '1' : '0');
    _iconeMudo();
    _sincronizarMudo(_ttsMudo);
    Som.envio();
    document.dispatchEvent(new CustomEvent('orion:tts', { detail: _ttsMudo }));
};

/* ==========================================================================
   SOM — tons sintetizados (Web Audio), sem arquivos
   O tom de boot pode não tocar na primeira abertura: o webview bloqueia
   áudio antes do primeiro gesto. Esperado.
   ========================================================================== */
const Som = (() => {
    let ctx = null;
    const obter = () => {
        if (!ctx) { try { ctx = new (window.AudioContext || window.webkitAudioContext)(); } catch (_) { ctx = null; } }
        return ctx;
    };
    document.addEventListener('pointerdown', () => obter()?.resume(), { once: true });
    document.addEventListener('keydown', () => obter()?.resume(), { once: true });

    function tom({ freq = 440, ate = null, dur = 0.3, tipo = 'sine', vol = 0.04, atraso = 0 }) {
        const c = obter();
        if (!c) return;
        const t0 = c.currentTime + atraso;
        const osc = c.createOscillator(), g = c.createGain();
        osc.type = tipo;
        osc.frequency.setValueAtTime(freq, t0);
        if (ate) osc.frequency.exponentialRampToValueAtTime(ate, t0 + dur);
        g.gain.setValueAtTime(0.0001, t0);
        g.gain.linearRampToValueAtTime(vol, t0 + 0.02);
        g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
        osc.connect(g).connect(c.destination);
        osc.start(t0);
        osc.stop(t0 + dur + 0.05);
    }
    return {
        boot() {      // quinta justa, grave — curto e sóbrio
            tom({ freq: 146.8, dur: 1.1, vol: 0.05 });
            tom({ freq: 220, dur: 0.9, vol: 0.03, atraso: 0.14 });
        },
        mensagem() { tom({ freq: 660, ate: 880, dur: 0.14, vol: 0.026 }); },
        envio()    { tom({ freq: 520, dur: 0.07, vol: 0.02 }); },
    };
})();

/* ==========================================================================
   BOOT — marca, checagem real do cérebro, constelação acende
   ========================================================================== */
const _esperar = ms => new Promise(r => setTimeout(r, ms));

async function _boot() {
    const overlay = document.getElementById('boot-overlay');
    const linhas = document.getElementById('boot-lines');
    if (!overlay || !linhas) { Ceu.revelar(); return; }
    const rapido = movimentoReduzido();

    const linha = texto => {
        const d = document.createElement('div');
        d.className = 'boot-line';
        d.textContent = texto;
        linhas.appendChild(d);
        requestAnimationFrame(() => d.classList.add('visible'));
        return d;
    };

    overlay.classList.add('brand-in');
    await _esperar(rapido ? 100 : 500);
    const l1 = linha('cérebro :8000 …');
    let online = false;
    try { await _fetchTimeout(CEREBRO + '/', 1500); online = true; } catch (_) {}
    l1.textContent = `cérebro :8000 · ${online ? 'ok' : 'offline'}`;
    l1.classList.toggle('bad', !online);
    _pontoStatus('cerebro', online);
    await _esperar(rapido ? 80 : 380);
    linha('pronto');
    await _esperar(rapido ? 80 : 420);

    Ceu.revelar();
    Som.boot();
    overlay.classList.add('hide');
    setTimeout(() => overlay.remove(), 700);
}

/* ==========================================================================
   INÍCIO
   ========================================================================== */
window.setOrionState = s => definirEstado(s);
window.setAudioIntensity = v => Ceu.setAudio(v);

definirEstado('idle');
Ceu.init(document.getElementById('canvas-container'));
_configurarEntrada();
_conectarHub();
_configurarStatus();
_configurarMetricas();
_configurarArrastar();
Voz.configurar();
_iconeMudo();
_sincronizarMudo(_ttsMudo);
_boot();
