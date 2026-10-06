"""Gera o ícone do Orion (assets/orion.ico e assets/orion.png) com Pillow.

Minimalista: a constelação de Órion (ombros, Cinturão e pés, ligados por fios finíssimos) num
quadrado arredondado quase preto; Betelgeuse é o único toque de cor. Sem brilho nem raios. Cada tamanho é desenhado do zero (não
reduzido do maior): nos pequenos os pontos ficam proporcionalmente maiores para não sumirem.

    uv run python scripts/gerar_icone.py
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw

TAMANHOS = (256, 128, 64, 48, 32, 24, 16)
SAIDA = Path(__file__).resolve().parent.parent / "assets"
SS = 8  # supersampling

FUNDO = (7, 10, 18)
BRANCO = (238, 242, 255)
ÂMBAR = (242, 179, 107)  # Betelgeuse: o único toque de cor

# Constelação de Órion como se vê do hemisfério sul/norte olhando para o sul: dois ombros, o
# Cinturão no meio (subindo para a direita) e dois pés. (x, y, tamanho relativo, cor)
ESTRELAS = {
    "betelgeuse": (0.30, 0.21, 1.25, ÂMBAR),
    "bellatrix": (0.71, 0.24, 0.9, BRANCO),
    "alnitak": (0.375, 0.555, 0.78, BRANCO),
    "alnilam": (0.50, 0.50, 0.88, BRANCO),
    "mintaka": (0.625, 0.445, 0.7, BRANCO),
    "saiph": (0.32, 0.81, 0.8, BRANCO),
    "rigel": (0.72, 0.78, 1.25, BRANCO),
}
LINHAS = (
    ("betelgeuse", "alnitak"), ("bellatrix", "mintaka"),
    ("alnitak", "saiph"), ("mintaka", "rigel"), ("alnitak", "alnilam"), ("alnilam", "mintaka"),
)  # fmt: skip
PEQUENO = ("betelgeuse", "alnitak", "alnilam", "mintaka", "rigel")  # em 24 px ou menos


def desenhar(tam: int) -> Image.Image:
    s = tam * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, s - 1, s - 1), radius=round(s * 0.225), fill=(*FUNDO, 255))
    nomes = PEQUENO if tam <= 24 else tuple(ESTRELAS)
    if tam >= 48:  # fios finíssimos: o desenho do guerreiro, sem pesar
        linha = max(1, round(s * 0.006))
        for a, b in LINHAS:
            (x0, y0, *_), (x1, y1, *_) = ESTRELAS[a], ESTRELAS[b]
            d.line((x0 * s, y0 * s, x1 * s, y1 * s), fill=(*BRANCO, 52), width=linha)
    base = 0.030 if tam >= 128 else 0.040 if tam >= 48 else 0.055 if tam >= 32 else 0.075
    for nome in nomes:
        x, y, k, cor = ESTRELAS[nome]
        r = base * k * s
        d.ellipse((x * s - r, y * s - r, x * s + r, y * s + r), fill=(*cor, 255))
    return img.resize((tam, tam), Image.Resampling.LANCZOS)


def main() -> None:
    SAIDA.mkdir(exist_ok=True)
    imagens = {t: desenhar(t) for t in TAMANHOS}
    imagens[256].save(SAIDA / "orion.png")
    imagens[256].save(
        SAIDA / "orion.ico",
        format="ICO",
        sizes=[(t, t) for t in TAMANHOS],
        append_images=[imagens[t] for t in TAMANHOS[1:]],
    )
    print("gerado:", SAIDA / "orion.ico", "e", SAIDA / "orion.png")


if __name__ == "__main__":
    main()
