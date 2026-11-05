"""Biblioteca de síntese (instrumentos e efeitos sonoros) compartilhada por music_ad.py e readme/music_slow.py."""
import numpy as np
from scipy.signal import butter, sosfilt, fftconvolve
SR = 44100
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
def ir(sec, seed):
    r = np.random.default_rng(seed); n = int(sec * SR); t = np.arange(n) / SR
    x = lp(r.standard_normal(n), 6000) * np.exp(-t * 6.9 / sec); x[:int(.012 * SR)] = 0; return x / np.sqrt((x ** 2).sum())
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

