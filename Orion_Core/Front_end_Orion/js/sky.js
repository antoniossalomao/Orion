/* ==========================================================================
   ORION — sky.js | a constelação de Órion em 3D (Three.js r128, tudo local)
   Posições reais (ascensão reta, declinação) e profundidade proporcional à distância.
     em espera   → oscila devagar (a profundidade aparece no movimento)
     ouvindo     → o cinturão acende com a intensidade do áudio
     processando → um traço percorre as linhas da figura
     respondendo → todas as estrelas pulsam com a voz
   Modos: 'home' (palco, 60 fps, paralaxe e rótulos) e 'fundo' (atrás das outras telas:
   ≤ 20 fps, pixel ratio 1, sem paralaxe) — a GPU não trabalha à toa por baixo de um chat opaco.
   ========================================================================== */
(function () {
    'use strict';
    const O = window.Orion;
    const { bus } = O;
    const PI = Math.PI;

    const COR = { idle: '#8fabff', listening: '#62e8c8', processing: '#f5b95f', speaking: '#eef2ff' };

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
    // as que têm nome próprio aparecem num rótulo ao passar o mouse
    const NOMES = {
        betelgeuse: ['Betelgeuse', 'Supergigante vermelha · ~550 anos-luz'],
        rigel:      ['Rigel', 'Supergigante azul · ~860 anos-luz'],
        bellatrix:  ['Bellatrix', 'A guerreira · ~250 anos-luz'],
        mintaka:    ['Mintaka', 'Cinturão de Órion · ~1.200 anos-luz'],
        alnilam:    ['Alnilam', 'Cinturão de Órion · ~1.340 anos-luz'],
        alnitak:    ['Alnitak', 'Cinturão de Órion · ~1.260 anos-luz'],
        saiph:      ['Saiph', 'Supergigante azul · ~650 anos-luz'],
        meissa:     ['Meissa', 'A cabeça · ~1.100 anos-luz'],
        m42:        ['Nebulosa de Órion', 'M42 · berçário de estrelas · ~1.340 anos-luz'],
    };
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
    const TRAJETO = ['meissa', 'betelgeuse', 'alnitak', 'saiph', 'alnitak', 'alnilam',
                     'mintaka', 'rigel', 'mintaka', 'bellatrix', 'meissa'];
    const CINTURAO = new Set(['alnitak', 'alnilam', 'mintaka']);

    const RA0 = 5.515, DEC0 = 5.3, ESCALA = 2.05, VEL_TRACO = 15;
    const FOV = 38, TAN = Math.tan((FOV / 2) * PI / 180);

    let scene, camera, renderer, root, fundo, cometa, nucleo, container;
    let estado = 'idle', alvoAudio = 0, audio = 0, flash = 1;
    let revelacao = -1, tracoS = 0, tracoOp = 0, segAtual = -1;
    let reduzido = O.movimentoReduzido();
    let modo = 'home', pronto = false, ultimoQuadro = 0, ultimoRender = 0, sujo = true;
    let paralaxeX = 0, paralaxeY = 0, alvoPX = 0, alvoPY = 0, deslocY = 0, camZ = 140, hoverId = null;
    const estrelas = new Map(), linhas = [], trajeto = [];
    let trajetoLen = 0;
    const _tmp = new THREE.Color();
    const TINT = { idle: null, listening: new THREE.Color(COR.listening), processing: new THREE.Color(COR.processing), speaking: new THREE.Color(COR.speaking) };
    const COR_LINHA = new THREE.Color(0x8fabff);

    /* texturas desenhadas em canvas — nenhum arquivo externo */
    function texRadial(paradas, sz = 64) {
        const cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const gr = ctx.createRadialGradient(sz / 2, sz / 2, 0, sz / 2, sz / 2, sz / 2);
        for (const [p, c] of paradas) gr.addColorStop(p, c);
        ctx.fillStyle = gr;
        ctx.fillRect(0, 0, sz, sz);
        return new THREE.CanvasTexture(cv);
    }
    let texEstrela, texHalo, texFlare;
    function criarTexturas() {
        texEstrela = texRadial([[0, 'rgba(255,255,255,1)'], [0.18, 'rgba(255,255,255,0.85)'], [0.45, 'rgba(255,255,255,0.16)'], [1, 'rgba(255,255,255,0)']]);
        texHalo = texRadial([[0, 'rgba(255,255,255,0.55)'], [0.35, 'rgba(255,255,255,0.12)'], [1, 'rgba(255,255,255,0)']], 128);
        const sz = 256, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        const raio = horizontal => {
            const gr = horizontal ? ctx.createLinearGradient(0, 0, sz, 0) : ctx.createLinearGradient(0, 0, 0, sz);
            gr.addColorStop(0, 'rgba(255,255,255,0)'); gr.addColorStop(0.5, 'rgba(255,255,255,0.9)'); gr.addColorStop(1, 'rgba(255,255,255,0)');
            ctx.fillStyle = gr;
            if (horizontal) ctx.fillRect(0, sz / 2 - 1, sz, 2); else ctx.fillRect(sz / 2 - 1, 0, 2, sz);
        };
        raio(true); raio(false);
        texFlare = new THREE.CanvasTexture(cv);
    }
    function texNebulosa(rgb, manchas = 5) {
        const sz = 256, cv = document.createElement('canvas');
        cv.width = cv.height = sz;
        const ctx = cv.getContext('2d');
        for (let i = 0; i < manchas; i++) {
            const x = sz * (0.32 + Math.random() * 0.36), y = sz * (0.32 + Math.random() * 0.36), r = sz * (0.18 + Math.random() * 0.2);
            const gr = ctx.createRadialGradient(x, y, 0, x, y, r);
            gr.addColorStop(0, `rgba(${rgb},0.5)`); gr.addColorStop(0.55, `rgba(${rgb},0.14)`); gr.addColorStop(1, `rgba(${rgb},0)`);
            ctx.fillStyle = gr;
            ctx.fillRect(0, 0, sz, sz);
        }
        return new THREE.CanvasTexture(cv);
    }
    function sprite(tex, cor, op, escala, aditivo = true) {
        const s = new THREE.Sprite(new THREE.SpriteMaterial({
            map: tex, color: cor, transparent: true, opacity: op, depthWrite: false,
            blending: aditivo ? THREE.AdditiveBlending : THREE.NormalBlending,
        }));
        s.scale.set(escala, escala, 1);
        return s;
    }
    function projetar(ra, dec, dist) {
        const x = -(ra - RA0) * 15 * Math.cos(DEC0 * PI / 180) * ESCALA;
        const y = (dec - DEC0) * ESCALA;
        const z = Math.max(-12, Math.min(12, -Math.log(dist / 700) * 9));
        return new THREE.Vector3(x, y, z);
    }

    function construirFigura() {
        root = new THREE.Group();
        root.position.y = 3;
        scene.add(root);
        const porBrilho = [...ESTRELAS].sort((a, b) => a[3] - b[3]).map(e => e[0]);
        for (const [id, ra, dec, mag, dist, cor] of ESTRELAS) {
            const pos = projetar(ra, dec, dist);
            const c = new THREE.Color(cor);
            const tam = 0.9 + (4.7 - mag) * 0.62;
            const g = new THREE.Group();
            g.position.copy(pos);
            const halo = sprite(texHalo, c.clone(), 0, tam * 3.2);
            const nucleoEstrela = sprite(texEstrela, c.clone(), 0, tam);
            g.add(halo); g.add(nucleoEstrela);
            let flare = null;
            if (mag < 0.6) { flare = sprite(texFlare, c.clone(), 0, tam * 5.5); g.add(flare); }
            if (id === 'm42') {
                const neb = sprite(texNebulosa('255,120,190', 4), 0xffffff, 0, 9);
                g.add(neb);
                g.userData.neb = neb;
            }
            root.add(g);
            estrelas.set(id, { id, pos, cor: c, tam, op: Math.min(1, 0.55 + (4.7 - mag) * 0.11),
                fase: Math.random() * PI * 2, ritmo: 0.8 + Math.random() * 1.6,
                ordem: porBrilho.indexOf(id), cinturao: CINTURAO.has(id), grupo: g, halo, nucleo: nucleoEstrela, flare });
        }
        LINHAS.forEach(([a, b, peso], i) => {
            const pa = estrelas.get(a).pos, pb = estrelas.get(b).pos;
            const geo = new THREE.BufferGeometry().setFromPoints([pa.clone(), pa.clone()]);
            const mat = new THREE.LineBasicMaterial({ color: COR_LINHA.clone(), transparent: true, opacity: 0, blending: THREE.AdditiveBlending, depthWrite: false });
            const linha = new THREE.Line(geo, mat);
            linha.frustumCulled = false;   // a geometria cresce durante a revelação
            root.add(linha);
            linhas.push({ a, b, pa, pb, base: 0.42 * peso, mat, geo, ordem: i, pronta: false, cinturao: CINTURAO.has(a) && CINTURAO.has(b) });
        });
        for (let i = 0; i < TRAJETO.length - 1; i++) {
            const a = estrelas.get(TRAJETO[i]).pos, b = estrelas.get(TRAJETO[i + 1]).pos;
            const len = a.distanceTo(b);
            const idx = linhas.findIndex(l => (l.a === TRAJETO[i] && l.b === TRAJETO[i + 1]) || (l.b === TRAJETO[i] && l.a === TRAJETO[i + 1]));
            trajeto.push({ a, b, len, linha: idx });
            trajetoLen += len;
        }
        cometa = new THREE.Group();
        for (let k = 0; k < 7; k++) {
            const s = sprite(texEstrela, new THREE.Color(COR.processing), 0, k === 0 ? 2.6 : 2.0 - k * 0.2);
            s.userData.k = k;
            cometa.add(s);
        }
        root.add(cometa);
        nucleo = sprite(texHalo, new THREE.Color(COR.idle), 0, 16);
        nucleo.position.copy(estrelas.get('alnilam').pos);
        nucleo.position.z -= 6;
        root.add(nucleo);
    }

    function construirFundo() {
        fundo = new THREE.Group();
        const camadas = [{ n: 160, size: 2.2, op: 0.7 }, { n: 600, size: 1.4, op: 0.45 }, { n: 1200, size: 0.9, op: 0.3 }];
        const tons = [new THREE.Color(0xdfe7ff), new THREE.Color(0xbfd0ff), new THREE.Color(0xffe9d2)];
        for (const L of camadas) {
            const pos = new Float32Array(L.n * 3), cor = new Float32Array(L.n * 3);
            for (let i = 0; i < L.n; i++) {
                const th = Math.random() * PI * 2, ph = Math.acos(2 * Math.random() - 1), r = 260 + Math.random() * 160;
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
                depthWrite: false, blending: THREE.AdditiveBlending, sizeAttenuation: true, alphaTest: 0.001 })));
        }
        scene.add(fundo);
        for (const d of [{ rgb: '40,70,150', pos: [-70, 30, -160], esc: 120, op: 0.05 }, { rgb: '80,40,110', pos: [80, -40, -190], esc: 140, op: 0.04 }]) {
            const s = sprite(texNebulosa(d.rgb), 0xffffff, d.op, d.esc, false);
            s.position.set(...d.pos);
            scene.add(s);
        }
    }

    function init(el) {
        container = el;
        try {
            renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'low-power' });
        } catch (e) {
            console.warn('[Orion] WebGL indisponível — céu estático.', e);
            container.dataset.fallback = 'true';
            return false;
        }
        scene = new THREE.Scene();
        camera = new THREE.PerspectiveCamera(FOV, 1, 0.1, 900);
        container.append(renderer.domElement);
        criarTexturas();
        construirFundo();
        construirFigura();
        pronto = true;
        resize();
        if (window.ResizeObserver) new ResizeObserver(resize).observe(container); else window.addEventListener('resize', resize);
        window.addEventListener('pointermove', aoMover, { passive: true });
        window.addEventListener('pointerleave', () => { alvoPX = alvoPY = 0; marcarHover(null); });
        renderer.domElement.addEventListener('webglcontextlost', e => { e.preventDefault(); pronto = false; container.dataset.fallback = 'true'; });
        requestAnimationFrame(quadro);
        return true;
    }

    function resize() {
        if (!pronto) return;
        const w = Math.max(1, container.clientWidth), h = Math.max(1, container.clientHeight);
        renderer.setPixelRatio(modo === 'home' ? Math.min(devicePixelRatio || 1, 1.75) : 1);
        renderer.setSize(w, h);
        camera.aspect = w / h;
        const mult = modo === 'home' && camera.aspect >= 0.8 ? 1.28 : 1;   // home: figura menor, sobra espaço para o conteúdo
        camZ = 140 * Math.max(1, 0.78 / camera.aspect) * mult;
        camera.position.z = camZ;
        camera.updateProjectionMatrix();
        sujo = true;
    }

    function pontoTrajeto(s) {
        s = ((s % trajetoLen) + trajetoLen) % trajetoLen;
        for (let i = 0; i < trajeto.length; i++) {
            const seg = trajeto[i];
            if (s <= seg.len) return { p: seg.a.clone().lerp(seg.b, s / seg.len), i };
            s -= seg.len;
        }
        return { p: trajeto[0].a.clone(), i: 0 };
    }

    /* ── interação: paralaxe e rótulos ─────────────────────────────────── */
    let moverAgendado = false, ultimoMove = null;
    function aoMover(e) {
        if (e.pointerType === 'touch' || modo !== 'home') return;
        ultimoMove = e;
        if (moverAgendado) return;
        moverAgendado = true;
        requestAnimationFrame(() => {
            moverAgendado = false;
            const ev = ultimoMove;
            if (!pronto || modo !== 'home') return;
            const r = container.getBoundingClientRect();
            alvoPX = reduzido ? 0 : Math.max(-1, Math.min(1, ((ev.clientX - r.left) / r.width - 0.5) * 2));
            alvoPY = reduzido ? 0 : Math.max(-1, Math.min(1, ((ev.clientY - r.top) / r.height - 0.5) * 2));
            if (ev.target.closest?.('.home-hero, #sidebar, #topbar, dialog, .dialog-scrim')) { marcarHover(null); return; }
            let melhor = null, dist = 18;
            root.updateMatrixWorld();
            for (const id of Object.keys(NOMES)) {
                const e2 = estrelas.get(id);
                const v = e2.pos.clone().applyMatrix4(root.matrixWorld).project(camera);
                const px = r.left + (v.x * 0.5 + 0.5) * r.width, py = r.top + (-v.y * 0.5 + 0.5) * r.height;
                const d = Math.hypot(px - ev.clientX, py - ev.clientY);
                if (d < dist) { dist = d; melhor = { id, x: px, y: py }; }
            }
            marcarHover(melhor);
        });
    }
    function marcarHover(h) {
        const id = h ? h.id : null;
        if (id === hoverId) return;
        hoverId = id;
        sujo = true;
        bus.emit('sky:hover', h ? { nome: NOMES[h.id][0], info: NOMES[h.id][1], x: h.x, y: h.y } : null);
    }

    /* ── laço ──────────────────────────────────────────────────────────── */
    let focado = true, pulo = 0;
    window.addEventListener('blur', () => { focado = false; });
    window.addEventListener('focus', () => { focado = true; });
    document.addEventListener('visibilitychange', () => { focado = !document.hidden; });

    function quadro(agora) {
        requestAnimationFrame(quadro);
        if (!pronto || document.hidden) return;
        if (!focado && (pulo = (pulo + 1) % 4) !== 0) return;
        // atrás de outra tela: 20 fps (1 fps se o movimento está reduzido e nada mudou)
        const intervalo = modo === 'home' ? 0 : (reduzido && !sujo ? 1000 : 50);
        if (agora - ultimoRender < intervalo - 2) return;
        ultimoRender = agora;

        const dt = Math.min(0.25, (agora - ultimoQuadro) / 1000 || 0.016);
        ultimoQuadro = agora;
        const t = agora / 1000;
        audio += (alvoAudio - audio) * 0.09;
        if (revelacao >= 0) revelacao += dt;
        if (flash < 1) flash = Math.min(1, flash + dt * 1.1);
        sujo = false;

        // câmera: paralaxe suave e deslocamento da figura para cima na home
        const alvoY = modo === 'home' ? 0.15 * (2 * camZ * TAN) : 0;
        deslocY += (alvoY - deslocY) * 0.06;
        paralaxeX += (alvoPX * 5 - paralaxeX) * 0.05;
        paralaxeY += (alvoPY * 3 - paralaxeY) * 0.05;
        root.position.y = 3 + deslocY;
        camera.position.x = paralaxeX;
        camera.position.y = -paralaxeY;
        camera.lookAt(0, deslocY * 0.6, 0);
        if (Math.abs(alvoY - deslocY) > 0.05) sujo = true;

        if (!reduzido) {
            root.rotation.y = Math.sin(t * 0.045) * 0.30;
            root.rotation.x = Math.sin(t * 0.031) * 0.05;
            fundo.rotation.y += dt * 0.004;
        } else root.rotation.set(0, 0, 0);

        const tracoAlvo = estado === 'processing' ? 1 : 0;
        tracoOp += (tracoAlvo - tracoOp) * 0.08;
        let cabeca = null;
        segAtual = -1;
        if (tracoOp > 0.01) {
            if (!reduzido) tracoS += dt * VEL_TRACO;
            const h = pontoTrajeto(tracoS);
            cabeca = h.p; segAtual = trajeto[h.i].linha;
            for (const s of cometa.children) {
                const k = s.userData.k;
                s.position.copy(k === 0 ? h.p : pontoTrajeto(tracoS - k * 0.9).p);
                s.material.opacity = tracoOp * (k === 0 ? 0.95 : 0.5 * (1 - k / 7));
            }
            sujo = true;
        } else for (const s of cometa.children) s.material.opacity = 0;

        const tint = TINT[estado];
        for (const e of estrelas.values()) {
            const r = revelacao < 0 ? 0 : Math.min(1, Math.max(0, (revelacao - e.ordem * 0.07) / 0.5));
            const tw = reduzido ? 1 : 0.88 + 0.12 * Math.sin(t * e.ritmo + e.fase);
            let boost = 0;
            if (estado === 'listening' && e.cinturao) boost = 0.35 + audio * 1.4;
            else if (estado === 'speaking') boost = audio * 0.9;
            if (cabeca) boost = Math.max(boost, Math.max(0, 1 - cabeca.distanceTo(e.pos) / 7) * 0.9 * tracoOp);
            if (e.id === hoverId) boost = Math.max(boost, 0.9);
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
                sujo = true;
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
        if (revelacao >= 0 && revelacao < 3) sujo = true;
        renderer.render(scene, camera);
    }

    const api = {
        init, resize,
        ativo: () => pronto,
        quadros: () => (renderer ? renderer.info.render.frame : 0),   // quadros desenhados até agora (testes)
        setEstado(s) { estado = s; sujo = true; if (s === 'processing') api.setAudio(0.3); else if (s === 'idle') setTimeout(() => { if (estado === 'idle') api.setAudio(0); }, 600); },
        setAudio(v) { alvoAudio = Math.max(0, Math.min(1, +v || 0)); sujo = true; },
        setModo(m) {
            if (m === modo) return;
            modo = m;
            if (m !== 'home') { alvoPX = alvoPY = 0; marcarHover(null); }
            resize();
        },
        revelar() { if (revelacao < 0) revelacao = reduzido ? 99 : 0; flash = 0; sujo = true; },
        setReduzido(v) { reduzido = !!v; if (v && revelacao >= 0) revelacao = 99; sujo = true; },
    };
    O.sky = api;
    bus.on('estado', s => api.setEstado(s));
    bus.on('audio', v => api.setAudio(v));
    bus.on('prefs', () => api.setReduzido(O.movimentoReduzido()));
})();
