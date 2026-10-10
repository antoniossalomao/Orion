"""Teclas globais da ponte: o mapa padrão e a leitura de `ORION_HOTKEYS` (JSON)."""

from __future__ import annotations

import json
import re

PADRAO: dict[str, str] = {
    "captura": "ctrl+alt+space",  # captura rápida (E2.3)
    "isso": "ctrl+alt+o",  # "o que é isso?" (E2.5)
    "ocr": "ctrl+alt+t",  # copiar texto de uma área da tela (E2.4)
    "panico": "ctrl+alt+shift+p",  # modo pânico (regra 48)
}
MODIFICADORES = ("ctrl", "alt", "shift", "cmd")
_TECLA = re.compile(
    r"^(?:[a-z0-9]|f(?:[1-9]|1[0-9]|20)|space|enter|tab|esc|home|end|up|down|left|right)$"
)


class TeclasInvalidas(ValueError):
    """`ORION_HOTKEYS` com JSON, ação ou combinação que a ponte não aceita."""


def normalizar(combo: str) -> str:
    """`Ctrl+Alt+Space` → `ctrl+alt+space`. Exige ao menos um modificador (tecla solta capturaria
    a digitação de tudo) e exatamente uma tecla final."""
    partes = [p.strip().lower() for p in combo.split("+") if p.strip()]
    mods = [p for p in partes if p in MODIFICADORES]
    teclas = [p for p in partes if p not in MODIFICADORES]
    if not mods:
        raise TeclasInvalidas(f"{combo!r}: use ao menos um modificador (ctrl, alt, shift, cmd)")
    if len(teclas) != 1 or not _TECLA.match(teclas[0]):
        raise TeclasInvalidas(f"{combo!r}: falta uma tecla válida no fim (a, 1, f5, space...)")
    return "+".join([*dict.fromkeys(mods), teclas[0]])


def mapa_de_teclas(bruto: str = "") -> dict[str, str]:
    """O padrão, com o que o `ORION_HOTKEYS` trocar. Ação desconhecida ou combinação repetida
    levanta `TeclasInvalidas` (a ponte não sobe com teclas ambíguas)."""
    mapa = dict(PADRAO)
    if bruto.strip():
        try:
            pedido = json.loads(bruto)
        except ValueError as e:
            raise TeclasInvalidas(f"ORION_HOTKEYS não é JSON: {e}") from None
        if not isinstance(pedido, dict):
            raise TeclasInvalidas('ORION_HOTKEYS precisa ser um objeto: {"captura": "ctrl+alt+k"}')
        for acao, combo in pedido.items():
            if acao not in PADRAO:
                raise TeclasInvalidas(f"ação desconhecida: {acao!r} (use {', '.join(PADRAO)})")
            if not isinstance(combo, str):
                raise TeclasInvalidas(f"{acao}: a combinação precisa ser texto")
            mapa[acao] = combo
    normal = {acao: normalizar(combo) for acao, combo in mapa.items()}
    if len(set(normal.values())) != len(normal):
        raise TeclasInvalidas("duas ações com a mesma combinação de teclas")
    return normal


def para_pynput(combo: str) -> str:
    """`ctrl+alt+space` → `<ctrl>+<alt>+<space>` (formato do `pynput.keyboard.GlobalHotKeys`)."""
    return "+".join(p if len(p) == 1 else f"<{p}>" for p in combo.split("+"))
