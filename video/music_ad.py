#!/usr/bin/env python3
"""Trilha estilo propaganda + efeitos sonoros, tudo sintetizado (sem direitos autorais).
Lê timeline.json (cenas) e sfx.json (efeitos exportados da cena por sfx_export.js).
Gera audio/music.wav (trilha) e audio/sfx.wav (efeitos). 120 BPM, Lá menor (Am-F-C-G),
drop no logo (s3 start + 1,5 s), breakdown nos números (s12) e acorde final no fecho (s13)."""
import json, wave, sys, numpy as np
from scipy.signal import butter, sosfilt, fftconvolve

SR = 44100
tl = json.load(open('timeline.json')); T = tl['total']; N = int(T * SR) + SR
S = {s['id']: s for s in tl['scenes']}
BEAT = 0.5; BAR = 2.0
# uso: python3 music_ad.py [cena_do_logo] [cena_do_breakdown] [cena_final] [atraso_do_drop]
A = sys.argv[1:] + ['s3', 's12', 's13', '1.5'][len(sys.argv[1:]):]
DROP = S[A[0]]['start'] + float(A[3])
BRK = S[A[1]]['start']; FIN = S[A[2]]['start']
rng = np.random.default_rng(7)

def mtof(m): return 440.0 * 2 ** ((m - 69) / 12)
def lp(x, f, o=2): return sosfilt(butter(o, f, 'low', fs=SR, output='sos'), x)
def hp(x, f, o=2): return sosfilt(butter(o, f, 'high', fs=SR, output='sos'), x)
def bp(x, a, b, o=2): return sosfilt(butter(o, [a, b], 'band', fs=SR, output='sos'), x)
def saw(f, n, ph=0.0): return 2 * ((f * np.arange(n) / SR + ph) % 1) - 1
def env_ad(n, a, d):
    t = np.arange(n) / SR; return np.minimum(1, t / max(a, 1e-4)) * np.exp(-np.maximum(0, t - a) / d)
def put(buf, x, t0, g=1.0):
    i = int(t0 * SR)
    if i >= len(buf) or i + len(x) <= 0: return
    if i < 0: x = x[-i:]; i = 0
    j = min(len(buf), i + len(x)); buf[i:j] += g * x[:j - i]

CH = [[57, 60, 64], [57, 60, 65], [55, 60, 64], [55, 59, 62]]   # Am F C G
ROOT = [33, 29, 36, 31]
def bar_idx(t): return int(np.floor((t - DROP) / BAR)) % 4

# ---------- instrumentos ----------
def kick():
    n = int(.42 * SR); t = np.arange(n) / SR
    f = 46 + 110 * np.exp(-t * 32); ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-t * 7.5) * 1.0 + hp(rng.standard_normal(n), 3000) * np.exp(-t * 300) * .25
def clap():
    n = int(.35 * SR); t = np.arange(n) / SR; nz = bp(rng.standard_normal(n), 900, 3200)
    e = sum(np.exp(-np.maximum(0, t - d) * 60) * (t >= d) for d in (0, .012, .024)) * .6 + np.exp(-t * 14) * (t >= .03) * .5
    return nz * e * .7
def hat(open_=False):
    n = int((.18 if open_ else .05) * SR); t = np.arange(n) / SR
    return hp(rng.standard_normal(n), 7500) * np.exp(-t * (14 if open_ else 90)) * .32
def snare():
    n = int(.18 * SR); t = np.arange(n) / SR
    return (bp(rng.standard_normal(n), 1500, 6000) * np.exp(-t * 28) * .6 + np.sin(2 * np.pi * 190 * t) * np.exp(-t * 30) * .4)
def crash():
    n = int(2.2 * SR); t = np.arange(n) / SR
    return hp(rng.standard_normal(n), 4500) * np.exp(-t * 2.2) * .22
def pluck(m, d=.22, bright=3200):
    n = int((d + .25) * SR); f = mtof(m)
    x = .6 * saw(f, n, rng.random()) + .4 * np.sign(np.sin(2 * np.pi * f * np.arange(n) / SR))
    return lp(x, bright) * env_ad(n, .003, d)

L = np.zeros(N); R = np.zeros(N)
padL = np.zeros(N); padR = np.zeros(N); bass = np.zeros(N); drums = np.zeros(N)
arpL = np.zeros(N); arpR = np.zeros(N); send = np.zeros(N)

# ---------- pad supersaw (claro/escuro com crossfade = varredura de filtro) ----------
t0 = DROP - BAR * np.ceil(DROP / BAR)          # primeiro compasso alinhado antes de 0
b = t0
while b < T:
    a = max(b, 0); n = int((b + BAR + .6 - a) * SR)
    if n > 0:
        c = CH[bar_idx(b + .01)]
        for side, buf in ((0, padL), (1, padR)):
            x = np.zeros(n)
            for m in c:
                for dt in (-.14, -.05, .05, .14):
                    x += saw(mtof(m) * 2 ** ((dt + (side - .5) * .04) / 12), n, rng.random())
            x /= 12
            tt = np.arange(n) / SR
            e = np.minimum(1, tt / .25) * np.minimum(1, np.maximum(0, (b + BAR + .6 - a) - tt) / .6)
            dark = lp(x, 700); bright = lp(x, 3800)
            mid = a + tt
            k = np.interp(mid, [0, 8, 16, DROP - .3, DROP, BRK, BRK + .1, FIN, T], [0, .1, .35, .9, .75, .75, .25, 1, .4])
            put(buf, (dark * (1 - k) + bright * k) * e, a)
    b += BAR
padgain = np.interp(np.arange(N) / SR, [0, 2, 8, DROP - .25, DROP - .2, DROP, BRK, FIN - .1, FIN, T - 3, T], [0, .45, .55, .75, .0, .9, .8, .7, 1.0, .6, 0])
padL *= padgain; padR *= padgain

# ---------- grade de batidas ----------
beats = np.arange(np.ceil(-DROP / BEAT) * BEAT + DROP, T, BEAT)
def section(t):
    if t < 8: return 'intro'
    if t < DROP - .25: return 'build'
    if t < DROP: return 'gap'
    if t < BRK: return 'main'
    if t < FIN: return 'brk'
    return 'fin'
K = kick(); C = clap(); HC = hat(); HO = hat(True); SN = snare(); CR = crash()
kick_times = []
for bt in beats:
    if bt < 0: continue
    sec = section(bt); bi = int(round((bt - DROP) / BEAT)) % 4
    if sec == 'build':
        if bt >= 12 and bi in (0,): put(drums, K, bt, .7); kick_times.append(bt)
        for s16 in range(4):
            put(drums, HC, bt + s16 * BEAT / 4, .25 + .35 * (bt - 8) / (DROP - 8))
    elif sec == 'main':
        put(drums, K, bt, 1.0); kick_times.append(bt)
        if bi in (1, 3): put(drums, C, bt, .9); put(send, C, bt, .25)
        put(drums, HO, bt + BEAT / 2, .55)
        for s16 in (1, 3): put(drums, HC, bt + s16 * BEAT / 4, .45)
        # virada a cada 8 compassos
        if int(round((bt - DROP) / BEAT)) % 32 == 31:
            for k in range(4): put(drums, SN, bt + k * BEAT / 4, .35 + .1 * k)
    elif sec == 'brk':
        for s16 in range(2): put(drums, HC, bt + s16 * BEAT / 2, .25)
    elif sec == 'fin':
        if bt < FIN + 6 and bt >= FIN:
            if int(round((bt - FIN) / BEAT)) % 4 == 0: put(drums, K, bt, .9); kick_times.append(bt)
            put(drums, HC, bt + BEAT / 2, .3)
# rufo de caixa antes do drop e antes do fecho
for (a, z) in ((DROP - 4, DROP - .25), (FIN - 2.5, FIN - .02)):
    t = a
    while t < z:
        p = (t - a) / (z - a); put(drums, SN, t, .15 + .55 * p ** 2); put(send, SN, t, .08)
        t += BEAT / (2 if p < .35 else 4 if p < .7 else 8)
# pratos nos cortes depois do drop
for s in tl['scenes']:
    if s['start'] >= DROP and s['id'] != A[1]: put(drums, CR, s['start'], .9); put(send, CR, s['start'], .2)
put(drums, CR, DROP, 1.4); put(send, CR, DROP, .5)

# ---------- baixo (colcheias, pluck + sub) ----------
for bt in beats:
    if bt < 8: continue
    sec = section(bt)
    if sec in ('gap', 'brk') : continue
    if sec == 'fin' and bt > FIN + 6: continue
    m = ROOT[bar_idx(bt + .01)]
    for e8 in (0, 1):
        tt = bt + e8 * BEAT / 2; n = int(.26 * SR); x = np.arange(n) / SR
        f = mtof(m + (12 if (e8 == 1 and sec == 'main') else 0))
        sub = np.sin(2 * np.pi * mtof(m) * x)
        tone = lp(saw(f, n), 380 if sec == 'build' else 650)
        g = .5 if sec == 'build' else .8
        put(bass, (sub * .7 + tone * .5) * env_ad(n, .004, .16), tt, g)

# ---------- arpejo (pluck com eco ping-pong) ----------
PAT = [0, 1, 2, 1, 3, 2, 1, 2]
for bt in beats:
    if bt < 1: continue
    sec = section(bt)
    if sec == 'gap': continue
    if sec == 'fin' and bt > FIN + 7: continue
    c = CH[bar_idx(bt + .01)]; notes = [c[0] + 12, c[1] + 12, c[2] + 12, c[0] + 24]
    steps = 2 if sec in ('intro', 'build') else 4
    gain = {'intro': .18, 'build': .25, 'main': .2, 'brk': .3, 'fin': .22}[sec]
    for k in range(steps):
        tt = bt + k * BEAT / steps; idx = int(round((tt - DROP) / (BEAT / 4))) % 8
        p = pluck(notes[PAT[idx] % 4], .16, 2600 if sec != 'main' else 3600) * gain
        put(arpL, p, tt, 1); put(arpR, p, tt, .7)
        put(arpR, p, tt + 3 * BEAT / 4, .38); put(arpL, p, tt + 6 * BEAT / 4, .2)
        put(send, p, tt, .25)

# ---------- acorde final e impacto ----------
n = int(7 * SR); x = np.zeros(n)
for m in (45, 57, 60, 64, 71, 76):
    for dt in (-.1, 0, .1): x += saw(mtof(m) * 2 ** (dt / 12), n, rng.random())
fin = lp(x / 18, 3000) * env_ad(n, .01, 2.6)
put(padL, fin, FIN, .9); put(padR, fin, FIN, .9); put(send, fin, FIN, .5)

# ---------- sidechain (bombeia o pad/baixo no bumbo) ----------
sc = np.ones(N); tt = np.arange(N) / SR
for kt in kick_times:
    i = int(kt * SR); j = min(N, i + int(.45 * SR)); x = (np.arange(j - i)) / SR
    sc[i:j] = np.minimum(sc[i:j], 1 - .62 * np.exp(-x / .11))
padL *= sc; padR *= sc; bass *= sc ** .7

# ---------- efeitos sonoros ----------
fxL = np.zeros(N); fxR = np.zeros(N); fxs = np.zeros(N)
def noise(n): return rng.standard_normal(n)
def sweep_bp(n, f0, f1, q=.5):
    # varredura de banda: mistura suave de 8 bandas fixas em escala log
    x = noise(n); p = np.arange(n) / max(1, n - 1); fc = f0 * (f1 / f0) ** p
    fs_ = np.geomspace(min(f0, f1), max(f0, f1), 8); out = np.zeros(n)
    for f in fs_:
        band = bp(x, f * (1 - q / 2), min(SR / 2 - 100, f * (1 + q / 2)))
        w = np.exp(-(np.log(fc / f) / .35) ** 2); out += band * w
    return out
def SFX(kind, d):
    if kind == 'drone':
        n = int(d * SR); t = np.arange(n) / SR
        x = np.sin(2 * np.pi * 55 * t) * .5 + np.sin(2 * np.pi * 82.4 * t) * .25 + lp(noise(n), 200) * .6
        return x * np.minimum(1, t / 2.5) * np.minimum(1, (d - t) / 1.2) * .35
    if kind == 'lid':
        n = int(1.6 * SR); t = np.arange(n) / SR
        sw = sweep_bp(n, 250, 900, .8) * np.sin(np.pi * np.minimum(1, t / 1.4)) ** 2 * .5
        cl = np.zeros(n); i = int(1.35 * SR); m = int(.05 * SR); cl[i:i + m] = bp(noise(m), 1500, 5000) * np.exp(-np.arange(m) / SR * 120) * .6
        th = np.zeros(n); m = int(.2 * SR); th[i:i + m] = np.sin(2 * np.pi * 90 * np.arange(m) / SR) * np.exp(-np.arange(m) / SR * 25) * .5
        return sw + cl + th
    if kind == 'boot':
        n = int(3.5 * SR); t = np.arange(n) / SR; x = np.zeros(n)
        for m, g in ((57, .5), (64, .4), (69, .35), (73, .3), (76, .25), (81, .15)):
            x += np.sin(2 * np.pi * mtof(m) * t + .3 * np.sin(2 * np.pi * 5 * t)) * g
        return x * np.minimum(1, t / .06) * np.exp(-t * 1.1) * .45 + np.sin(2 * np.pi * 55 * t) * np.exp(-t * 4) * .5
    if kind in ('swish', 'whoosh', 'whooshlong'):
        dur = {'swish': .38, 'whoosh': .75}.get(kind, max(d, 1.0)); n = int(dur * SR); t = np.arange(n) / SR
        e = np.sin(np.pi * np.clip(t / dur, 0, 1)) ** 2.5
        return sweep_bp(n, 400 if kind != 'swish' else 900, 5000 if kind != 'whooshlong' else 2500, .9) * e * (1.1 if kind == 'swish' else .9)
    if kind == 'suck':
        n = int(.55 * SR); t = np.arange(n) / SR
        return sweep_bp(n, 4000, 300, .9) * (t / .55) ** 2.2 * 1.2
    if kind == 'riser':
        dur = max(d, .5); n = int(dur * SR); t = np.arange(n) / SR; p = t / dur
        ph = 2 * np.pi * np.cumsum(200 + 1600 * p ** 2) / SR
        return (sweep_bp(n, 300, 8000, .7) * .8 + np.sin(ph) * .15) * p ** 2.2
    if kind == 'pop':
        n = int(.09 * SR); t = np.arange(n) / SR; f = 520 + 500 * np.exp(-t * 60)
        return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 45) * .6
    if kind in ('thump', 'hit'):
        n = int(.35 * SR); t = np.arange(n) / SR; f = 60 + 90 * np.exp(-t * 40)
        x = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 14) * .9
        if kind == 'hit': x += bp(noise(n), 800, 5000) * np.exp(-t * 25) * .5
        return x
    if kind == 'scratch':
        n = int(.4 * SR); t = np.arange(n) / SR
        return bp(noise(n), 1200, 4000) * (.6 + .4 * np.sign(np.sin(2 * np.pi * 38 * t))) * np.minimum(1, t / .02) * np.exp(-t * 4) * .6
    if kind == 'impact':
        n = int(3.2 * SR); t = np.arange(n) / SR; f = 38 + 70 * np.exp(-t * 6)
        return (np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 1.6) * 1.0 + lp(noise(n), 1800) * np.exp(-t * 4) * .7
                + hp(noise(n), 3000) * np.exp(-t * 9) * .25)
    if kind == 'sub':
        n = int(2.0 * SR); t = np.arange(n) / SR; f = 55 - 20 * np.minimum(1, t / 1.5)
        return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 1.4) * .9
    if kind == 'plink':
        n = int(1.0 * SR); t = np.arange(n) / SR; m = [88, 91, 93][int(d) % 3]
        return (np.sin(2 * np.pi * mtof(m) * t) + .3 * np.sin(2 * np.pi * mtof(m) * 2.01 * t)) * np.exp(-t * 5) * .3
    if kind == 'shimmer':
        n = int(2.6 * SR); t = np.arange(n) / SR; x = np.zeros(n)
        for k in range(14):
            f = mtof(81 + [0, 3, 7, 10, 12, 15, 19][k % 7] + 12 * (k // 7)); x += np.sin(2 * np.pi * f * t + k) * (.5 + .5 * np.sin(2 * np.pi * (3 + k * .7) * t))
        return x / 14 * np.minimum(1, t / .3) * np.exp(-t * 1.3) * .5
    if kind == 'tick':
        n = int(.02 * SR); return hp(noise(n), 4000) * np.exp(-np.arange(n) / SR * 400) * .5
    if kind == 'click':
        n = int(.09 * SR); x = np.zeros(n)
        for o in (0, .045):
            i = int(o * SR); m = int(.012 * SR); tt = np.arange(m) / SR
            x[i:i + m] += (bp(noise(m), 2000, 7000) * .6 + np.sin(2 * np.pi * 2400 * tt) * .4) * np.exp(-tt * 500)
        return x
    if kind == 'ding':
        n = int(1.4 * SR); t = np.arange(n) / SR
        return (np.sin(2 * np.pi * mtof(84) * t) * .5 + np.sin(2 * np.pi * mtof(88) * t) * np.exp(-np.maximum(0, t - .08) * 3) * (t > .08) * .5) * np.exp(-t * 3) * .45
    if kind == 'ping':
        n = int(.6 * SR); t = np.arange(n) / SR
        f = np.where(t < .1, mtof(88), mtof(93))
        return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 7) * .4
    if kind == 'sparkle':
        n = int(d * SR); x = np.zeros(n); tt = 0.0; k = 0
        while tt < d - .3:
            m = 84 + [0, 3, 5, 7, 10, 12, 15][int(rng.random() * 7)]; nn = int(.35 * SR); t = np.arange(nn) / SR
            put(x, np.sin(2 * np.pi * mtof(m) * t) * np.exp(-t * 14) * .12, tt)
            tt += .22 * (1 - .75 * tt / d) + .02 * rng.random(); k += 1
        return x
    if kind == 'pulse':
        n = int(2.0 * SR); t = np.arange(n) / SR
        return (np.sin(2 * np.pi * 220 * t) * np.exp(-t * 3) * .5 + np.sin(2 * np.pi * 55 * t) * np.exp(-t * 6) * .6)
    if kind == 'lock':
        n = int(.25 * SR); t = np.arange(n) / SR; x = bp(noise(n), 2500, 8000) * np.exp(-t * 200) * .5
        x += np.sin(2 * np.pi * 130 * t) * np.exp(-t * 35) * .5; i = int(.05 * SR); x[i:] += bp(noise(n - i), 3000, 9000) * np.exp(-t[:n - i] * 300) * .4
        return x
    if kind == 'key':
        n = int(.12 * SR); t = np.arange(n) / SR
        return bp(noise(n), 1800, 6000) * np.exp(-t * 180) * .6 + np.sin(2 * np.pi * 140 * t) * np.exp(-t * 60) * .7
    if kind == 'ticks':
        n = int(d * SR); x = np.zeros(n); tt = 0.0; c = SFX('tick', 0)
        while tt < d: put(x, c, tt, .6); tt += .035 + .2 * (tt / d) ** 2
        return x
    raise ValueError(kind)

for (t0, kind, g, d) in json.load(open('sfx.json')):
    x = SFX(kind, d) * g
    pan = {'swish': .35, 'whoosh': .2, 'pop': .5, 'plink': .4, 'key': .2, 'sparkle': .6}.get(kind, 0)
    side = 1 if int(t0 * 1000) % 2 else -1
    put(fxL, x, t0, 1 - pan * max(0, side)); put(fxR, x, t0, 1 - pan * max(0, -side))
    if kind in ('impact', 'boot', 'plink', 'shimmer', 'ding', 'ping', 'pulse', 'hit', 'sparkle'): put(fxs, x, t0, .5)

# ---------- reverb (IR de ruído com decaimento) ----------
def ir(sec, seed):
    r = np.random.default_rng(seed); n = int(sec * SR); t = np.arange(n) / SR
    x = lp(r.standard_normal(n), 6000) * np.exp(-t * 6.9 / sec); x[:int(.012 * SR)] = 0; return x / np.sqrt((x ** 2).sum())
I1, I2 = ir(2.4, 1), ir(2.4, 2)
mus_send = send + (padL + padR) * .15
rvL = fftconvolve(mus_send, I1)[:N] * .9; rvR = fftconvolve(mus_send, I2)[:N] * .9
fxrL = fftconvolve(fxs, I1)[:N] * .8; fxrR = fftconvolve(fxs, I2)[:N] * .8

mL = padL * .55 + bass * .75 + drums * .8 + arpL * .9 + rvL
mR = padR * .55 + bass * .75 + drums * .8 + arpR * .9 + rvR
fade = np.interp(np.arange(N) / SR, [0, T - 2.5, T], [1, 1, 0])
def write(name, Lc, Rc, peak):
    st = np.stack([Lc, Rc], 1)[:int(T * SR)] * fade[:int(T * SR), None]
    st = st / max(1e-9, np.abs(st).max()) * peak
    w = wave.open(name, 'wb'); w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR)
    w.writeframes((st * 32767).astype('<i2').tobytes()); w.close()
write('audio/music.wav', mL, mR, .89)
write('audio/sfx.wav', fxL + fxrL, fxR + fxrR, .89)
print('ok', T, 'drop', DROP)
