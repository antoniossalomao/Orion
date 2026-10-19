#!/usr/bin/env python3
"""Junta, lado a lado, a foto do protótipo (esquerda) e a do app iced (direita), por cena.
Uso: comparar.py <pasta-prototipo> <pasta-iced> <pasta-saida> [filtro]"""
import sys, os, glob
from PIL import Image, ImageDraw
a, b, out = sys.argv[1:4]
filtro = sys.argv[4] if len(sys.argv) > 4 else ""
os.makedirs(out, exist_ok=True)
for fa in sorted(glob.glob(os.path.join(a, "*.png"))):
    n = os.path.basename(fa)
    if filtro not in n:
        continue
    fb = os.path.join(b, n)
    if not os.path.exists(fb):
        continue
    ia, ib = Image.open(fa).convert("RGB"), Image.open(fb).convert("RGB")
    ib = ib.resize(ia.size, Image.LANCZOS)
    im = Image.new("RGB", (ia.width * 2 + 8, ia.height), (40, 40, 40))
    im.paste(ia, (0, 0)); im.paste(ib, (ia.width + 8, 0))
    im.save(os.path.join(out, n))
    print(n)
