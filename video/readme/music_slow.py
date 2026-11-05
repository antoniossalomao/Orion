#!/usr/bin/env python3
"""Trilha lenta (84 BPM, Lá menor Am-F-C-G) + efeitos para o vídeo do README.
Lê sfx.json (de ../sfx_export.js) e a duração total como argumento; grava audio_readme.wav (mix final, ~-16 LUFS).
Uso: python3 music_slow.py <duração_total_s>"""
import sys, json, wave, subprocess, numpy as np
sys.path.insert(0, '..')
from sfx_lib import SR, rng, mtof, lp, hp, bp, saw, env_ad, put, ir, SFX
from scipy.signal import fftconvolve

T = float(sys.argv[1]); N = int(T * SR) + SR
SLOW = 1.3
def S(b): return b * SLOW                          # tempo da cena (base) -> tempo do vídeo
GROOVE, BRK, FIN = S(4.2), S(48.6), S(54.0)        # entra o ritmo no app real; pausa no trio; acorde no fecho
BEAT = 60 / 84; BAR = 4 * BEAT
CH = [[57, 60, 64], [57, 60, 65], [55, 60, 64], [55, 59, 62]]
ROOT = [33, 29, 36, 31]
def chord_at(t): return int(np.floor(t / BAR)) % 4

padL = np.zeros(N); padR = np.zeros(N); bass = np.zeros(N); drums = np.zeros(N); arpL = np.zeros(N); arpR = np.zeros(N); send = np.zeros(N)

# pad respirando (filtro abre e fecha devagar)
b = 0.0
while b < T:
    n = int((BAR + 1.2) * SR); c = CH[chord_at(b + .01)]
    for side, buf in ((0, padL), (1, padR)):
        x = np.zeros(n)
        for m in c + [c[0] - 12]:
            for dt in (-.1, .1):
                x += saw(mtof(m) * 2 ** ((dt + (side - .5) * .05) / 12), n, rng.random())
        x /= 8; tt = np.arange(n) / SR
        e = np.minimum(1, tt / .8) * np.minimum(1, np.maximum(0, BAR + 1.2 - tt) / 1.2)
        k = .5 + .5 * np.sin(2 * np.pi * (b + tt) / 11)
        put(buf, (lp(x, 600) * (1 - k) + lp(x, 2200) * k) * e, b)
    b += BAR
g = np.interp(np.arange(N) / SR, [0, 2.5, GROOVE, BRK, BRK + 1, FIN - .2, FIN, T - 3, T], [0, .5, .6, .6, .75, .75, 1, .6, 0])
padL *= g; padR *= g

def kick():
    n = int(.5 * SR); t = np.arange(n) / SR; f = 44 + 80 * np.exp(-t * 26)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 6) * .9
def rim():
    n = int(.12 * SR); t = np.arange(n) / SR
    return (bp(rng.standard_normal(n), 1800, 5000) * np.exp(-t * 60) * .5 + np.sin(2 * np.pi * 820 * t) * np.exp(-t * 80) * .3)
def shaker():
    n = int(.09 * SR); t = np.arange(n) / SR
    return hp(rng.standard_normal(n), 6000) * np.sin(np.pi * np.minimum(1, t / .09)) ** 2 * .18
def pluck(m, d=.35):
    n = int((d + .3) * SR); f = mtof(m); x = saw(f, n, rng.random()) * .5 + np.sin(2 * np.pi * f * np.arange(n) / SR) * .5
    return lp(x, 2400) * env_ad(n, .004, d)
K, RM, SH = kick(), rim(), shaker()
kicks = []
bt = 0.0; i = 0
while bt < T:
    sec = 'intro' if bt < GROOVE else 'brk' if BRK <= bt < FIN else 'fin' if bt >= FIN else 'main'
    pos = i % 4
    if sec == 'main':
        if pos in (0,) or (pos == 2 and (i // 4) % 2): put(drums, K, bt, .8); kicks.append(bt)
        if pos == 2: put(drums, RM, bt, .55); put(send, RM, bt, .2)
        put(drums, SH, bt + BEAT / 2, .9); put(drums, SH, bt, .5)
    elif sec == 'fin' and bt < FIN + 4 * BEAT and pos == 0:
        put(drums, K, bt, .8); kicks.append(bt)
    # baixo: nota longa por compasso, pulsando no 1 e no 3
    if sec in ('main', 'fin') and pos in (0, 2) and bt < T - 3:
        m = ROOT[chord_at(bt + .01)]; n = int(BEAT * 2 * SR); x = np.arange(n) / SR
        put(bass, (np.sin(2 * np.pi * mtof(m) * x) * .8 + lp(saw(mtof(m), n), 300) * .3) * env_ad(n, .02, .9), bt, .6)
    # arpejo em colcheias com eco
    if bt > .5 and sec != 'fin' or (sec == 'fin' and bt < FIN + 6):
        c = CH[chord_at(bt + .01)]; notes = [c[0] + 12, c[2] + 12, c[1] + 24, c[2] + 12]
        for k in (0, 1):
            tt = bt + k * BEAT / 2; idx = (i * 2 + k) % 4
            gain = .16 if sec == 'intro' else .22 if sec == 'brk' else .18
            p = pluck(notes[idx]) * gain
            put(arpL, p, tt); put(arpR, p, tt, .6); put(arpR, p, tt + .75 * BEAT, .35); put(arpL, p, tt + 1.5 * BEAT, .2); put(send, p, tt, .35)
    bt += BEAT; i += 1

# acorde final
n = int(8 * SR); x = np.zeros(n)
for m in (45, 57, 60, 64, 71, 76):
    for dt in (-.08, .08): x += saw(mtof(m) * 2 ** (dt / 12), n, rng.random())
fin = lp(x / 12, 2600) * env_ad(n, .02, 3.0)
put(padL, fin, FIN + S(1.8), .8); put(padR, fin, FIN + S(1.8), .8); put(send, fin, FIN + S(1.8), .5)

# sidechain suave
sc = np.ones(N)
for kt in kicks:
    a = int(kt * SR); z = min(N, a + int(.6 * SR)); x = np.arange(z - a) / SR
    sc[a:z] = np.minimum(sc[a:z], 1 - .4 * np.exp(-x / .16))
padL *= sc; padR *= sc; bass *= sc ** .6

# efeitos
fxL = np.zeros(N); fxR = np.zeros(N); fxs = np.zeros(N)
for (t0, kind, gg, d) in json.load(open('sfx.json')):
    x = SFX(kind, d) * gg; side = 1 if int(t0 * 1000) % 2 else -1
    pan = {'swish': .35, 'whoosh': .2, 'pop': .5, 'plink': .4, 'key': .2, 'sparkle': .6, 'tick': .4}.get(kind, 0)
    put(fxL, x, t0, 1 - pan * max(0, side)); put(fxR, x, t0, 1 - pan * max(0, -side))
    if kind in ('impact', 'boot', 'plink', 'shimmer', 'ding', 'ping', 'pulse', 'hit', 'sparkle', 'pop'): put(fxs, x, t0, .45)

I1, I2 = ir(3.2, 1), ir(3.2, 2)
ms = send + (padL + padR) * .2
L = padL * .6 + bass * .8 + drums * .75 + arpL + fftconvolve(ms, I1)[:N] + fxL * .9 + fftconvolve(fxs, I1)[:N] * .8
R = padR * .6 + bass * .8 + drums * .75 + arpR + fftconvolve(ms, I2)[:N] + fxR * .9 + fftconvolve(fxs, I2)[:N] * .8
fade = np.interp(np.arange(N) / SR, [0, T - 2.5, T], [1, 1, 0])
st = (np.stack([L, R], 1) * fade[:, None])[:int(T * SR)]; st = st / np.abs(st).max() * .9
w = wave.open('build/raw.wav', 'wb'); w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR); w.writeframes((st * 32767).astype('<i2').tobytes()); w.close()
subprocess.check_call(['ffmpeg', '-y', '-v', 'error', '-i', 'build/raw.wav', '-af', 'loudnorm=I=-16:TP=-1.2:LRA=11,aresample=44100', 'audio_readme.wav'])
print('audio_readme.wav', T)
