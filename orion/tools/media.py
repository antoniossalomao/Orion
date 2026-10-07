"""Mídia e janelas do `orion-desktop` (fase 4): `controlar_midia` e `controlar_janela`.

`controlar_midia` (escrita com log) aperta as teclas de mídia do sistema: tocar/pausar, próxima,
anterior, volume e mudo. `controlar_janela` (execução: confirma sempre) lista, foca, minimiza,
maximiza, restaura e **fecha** janelas; fechar pode perder o que não foi salvo, então nada aqui
roda sem o aval do Antônio.

- Windows: PowerShell (teclas de mídia por `keybd_event`; janelas por `user32` e `Get-Process`).
- Linux: `playerctl` e `wpctl`/`pactl`/`amixer` (mídia); `wmctrl` (janelas, só sessão X11).
- macOS: mídia por `osascript` (Spotify ou Music, volume do sistema). Janelas **não** têm
  suporte: exigiriam a permissão de Acessibilidade e um AppleScript por programa.

O texto do modelo (o título da janela) nunca entra numa linha de comando: no Windows vai por
variável de ambiente que o script lê, no Linux a janela é achada por Python e o `wmctrl` recebe só
o id hexadecimal. Uma busca que acha mais de uma janela **não age** (devolve a lista): agir na
"primeira" poderia fechar a janela errada. Os títulos que voltam são conteúdo externo (o título de
uma aba pode trazer instrução escrita), então a ferramenta é marcada como externa na política.

Só o argv foi testado fora do sistema de cada um (ver ORION_MELHORIAS).
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from collections.abc import Callable
from typing import Any

from .desktop import env_limpo
from .registry import Tool
from .system_tools import _decodificar, _script_ps

ACOES_MIDIA = ("tocar_pausar", "proxima", "anterior", "volume_mais", "volume_menos", "mudo")
ACOES_JANELA = ("listar", "focar", "minimizar", "maximizar", "restaurar", "fechar")
_VK = {  # teclas de mídia do Windows
    "tocar_pausar": 179,
    "proxima": 176,
    "anterior": 177,
    "volume_mais": 175,
    "volume_menos": 174,
    "mudo": 173,
}
_PS_MIDIA = (
    "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; "
    'public class OrionTecla { [DllImport("user32.dll")] '
    "public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra); }';"
    "$vk = [byte][int]$env:MIDIA_VK; $n = [int]$env:MIDIA_VEZES;"
    "1..$n | ForEach-Object {"
    "[OrionTecla]::keybd_event($vk, 0, 0, [UIntPtr]::Zero);"
    "[OrionTecla]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)}"
)
_PS_JANELA = (
    "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; "
    'public class OrionJanela { [DllImport("user32.dll")] '
    'public static extern bool ShowWindow(IntPtr h, int c); [DllImport("user32.dll")] '
    "public static extern bool SetForegroundWindow(IntPtr h); }';"
    "$acao = $env:JANELA_ACAO; $alvo = $env:JANELA_TITULO;"
    "$todas = @(Get-Process | Where-Object { $_.MainWindowTitle });"
    "if ($acao -eq 'listar') { $todas | ForEach-Object "
    "{ '{0}|{1}' -f $_.ProcessName, $_.MainWindowTitle }; exit 0 };"
    "$achadas = @($todas | Where-Object "
    "{ $_.MainWindowTitle.IndexOf($alvo, [StringComparison]::OrdinalIgnoreCase) -ge 0 });"
    "if ($achadas.Count -eq 0) { [Console]::Error.WriteLine('janela não encontrada'); exit 2 };"
    "if ($achadas.Count -gt 1) { [Console]::Error.WriteLine('mais de uma janela: seja específico');"
    "$achadas | ForEach-Object { '{0}|{1}' -f $_.ProcessName, $_.MainWindowTitle }; exit 3 };"
    "$p = $achadas[0]; $h = $p.MainWindowHandle;"
    "if ($acao -eq 'focar') { [OrionJanela]::ShowWindow($h, 9) | Out-Null;"
    " [OrionJanela]::SetForegroundWindow($h) | Out-Null }"
    "elseif ($acao -eq 'minimizar') { [OrionJanela]::ShowWindow($h, 6) | Out-Null }"
    "elseif ($acao -eq 'maximizar') { [OrionJanela]::ShowWindow($h, 3) | Out-Null }"
    "elseif ($acao -eq 'restaurar') { [OrionJanela]::ShowWindow($h, 9) | Out-Null }"
    "elseif ($acao -eq 'fechar') { $p.CloseMainWindow() | Out-Null };"
    "'{0}|{1}' -f $p.ProcessName, $p.MainWindowTitle"
)
_MAC_APPS = (
    'tell application "System Events" to set apps to name of processes\n'
    'if apps contains "Spotify" then\n tell application "Spotify" to {verbo}\n'
    'else if apps contains "Music" then\n tell application "Music" to {verbo}\n'
    "end if"
)
_MAC_MIDIA = {
    "tocar_pausar": _MAC_APPS.format(verbo="playpause"),
    "proxima": _MAC_APPS.format(verbo="next track"),
    "anterior": _MAC_APPS.format(verbo="previous track"),
    "volume_mais": "set volume output volume ((output volume of (get volume settings)) + 10)",
    "volume_menos": "set volume output volume ((output volume of (get volume settings)) - 10)",
    "mudo": "set volume output muted (not (output muted of (get volume settings)))",
}
_ID_JANELA = re.compile(r"^0x[0-9a-fA-F]+$")
_WMCTRL_B = {
    "minimizar": ["add,hidden"],
    "maximizar": ["add,maximized_vert,maximized_horz"],
    "restaurar": ["remove,hidden,maximized_vert,maximized_horz"],
}


def argv_midia(
    acao: str, platform: str, which: Callable[[str], str | None]
) -> tuple[list[str], dict[str, str]] | None:
    """Comando que executa `acao` de mídia; None se o sistema não tem como."""
    if platform == "win32":
        exe = which("pwsh") or which("powershell")
        if exe is None:
            return None
        vezes = 5 if acao.startswith("volume") else 1  # cada tecla de volume vale 2%
        return _script_ps(_PS_MIDIA, {"MIDIA_VK": str(_VK[acao]), "MIDIA_VEZES": str(vezes)}, exe)
    if platform == "darwin":
        return ["osascript", "-e", _MAC_MIDIA[acao]], env_limpo()
    if acao in ("tocar_pausar", "proxima", "anterior"):
        if which("playerctl") is None:
            return None
        verbo = {"tocar_pausar": "play-pause", "proxima": "next", "anterior": "previous"}[acao]
        return ["playerctl", verbo], env_limpo()
    if which("wpctl"):
        sink = "@DEFAULT_AUDIO_SINK@"
        por = {
            "volume_mais": ["set-volume", sink, "5%+"],
            "volume_menos": ["set-volume", sink, "5%-"],
            "mudo": ["set-mute", sink, "toggle"],
        }[acao]
        return ["wpctl", *por], env_limpo()
    if which("pactl"):
        por = {
            "volume_mais": ["set-sink-volume", "@DEFAULT_SINK@", "+5%"],
            "volume_menos": ["set-sink-volume", "@DEFAULT_SINK@", "-5%"],
            "mudo": ["set-sink-mute", "@DEFAULT_SINK@", "toggle"],
        }[acao]
        return ["pactl", *por], env_limpo()
    if which("amixer"):
        por = {"volume_mais": "5%+", "volume_menos": "5%-", "mudo": "toggle"}[acao]
        return ["amixer", "-q", "sset", "Master", por], env_limpo()
    return None


def media_tools(
    *,
    platform: str = sys.platform,
    runner: Callable[..., Any] = subprocess.run,
    which: Callable[[str], str | None] = shutil.which,
    timeout_s: float = 15.0,
) -> list[Tool]:
    def _rodar(argv: list[str], env: dict[str, str]) -> subprocess.CompletedProcess | str:
        try:
            return runner(
                argv, env=env, stdin=subprocess.DEVNULL,
                capture_output=True, timeout=timeout_s, check=False,
            )  # fmt: skip
        except (OSError, subprocess.TimeoutExpired) as e:
            return f"falhou: {type(e).__name__}"

    # ── mídia ─────────────────────────────────────────────────────────────────────
    def controlar_midia(acao: str) -> dict[str, Any]:
        if acao not in ACOES_MIDIA:
            return {"ok": False, "erro": f"ação inválida: {acao}. Use: {', '.join(ACOES_MIDIA)}"}
        comando = argv_midia(acao, platform, which)
        if comando is None:
            return {
                "ok": False,
                "erro": "nenhum comando de mídia (no Linux instale playerctl e wpctl/pactl)",
            }
        r = _rodar(*comando)
        if isinstance(r, str):
            return {"ok": False, "erro": r}
        return {"ok": r.returncode == 0, "acao": acao, "erro": _decodificar(r.stderr)[:200] or None}

    # ── janelas ───────────────────────────────────────────────────────────────────
    def _janelas_wmctrl() -> list[tuple[str, str]] | str:
        r = _rodar(["wmctrl", "-l"], env_limpo())
        if isinstance(r, str):
            return r
        if r.returncode != 0:
            return "wmctrl falhou (precisa de sessão X11): " + _decodificar(r.stderr)[:150]
        achadas: list[tuple[str, str]] = []
        for linha in _decodificar(r.stdout).splitlines():
            partes = linha.split(None, 3)
            if len(partes) == 4 and _ID_JANELA.match(partes[0]):
                achadas.append((partes[0], partes[3]))
        return achadas

    def _janela_linux(acao: str, titulo: str) -> dict[str, Any]:
        if which("wmctrl") is None:
            return {"ok": False, "erro": "wmctrl não encontrado (só funciona em sessão X11)"}
        janelas = _janelas_wmctrl()
        if isinstance(janelas, str):
            return {"ok": False, "erro": janelas}
        if acao == "listar":
            return {"ok": True, "janelas": [t for _, t in janelas][:100]}
        achadas = [(i, t) for i, t in janelas if titulo.lower() in t.lower()]
        if not achadas:
            return {"ok": False, "erro": "janela não encontrada"}
        if len(achadas) > 1:
            return {
                "ok": False,
                "erro": "mais de uma janela combina: seja mais específico",
                "janelas": [t for _, t in achadas][:20],
            }
        ident, nome = achadas[0]
        if acao == "focar":
            argv = ["wmctrl", "-i", "-a", ident]
        elif acao == "fechar":
            argv = ["wmctrl", "-i", "-c", ident]
        else:
            argv = ["wmctrl", "-i", "-r", ident, "-b", *_WMCTRL_B[acao]]
        r = _rodar(argv, env_limpo())
        if isinstance(r, str):
            return {"ok": False, "erro": r}
        return {"ok": r.returncode == 0, "acao": acao, "janela": nome}

    def _janela_windows(acao: str, titulo: str) -> dict[str, Any]:
        exe = which("pwsh") or which("powershell")
        if exe is None:
            return {"ok": False, "erro": "PowerShell não encontrado"}
        argv, env = _script_ps(_PS_JANELA, {"JANELA_ACAO": acao, "JANELA_TITULO": titulo}, exe)
        r = _rodar(argv, env)
        if isinstance(r, str):
            return {"ok": False, "erro": r}
        linhas = [ln for ln in _decodificar(r.stdout).splitlines() if ln.strip()]
        if acao == "listar":
            return {"ok": r.returncode == 0, "janelas": linhas[:100]}
        if r.returncode != 0:
            erro = _decodificar(r.stderr)[:200] or "falhou"
            return {"ok": False, "erro": erro, **({"janelas": linhas[:20]} if linhas else {})}
        return {"ok": True, "acao": acao, "janela": linhas[0] if linhas else ""}

    def controlar_janela(acao: str, titulo: str = "") -> dict[str, Any]:
        if acao not in ACOES_JANELA:
            return {"ok": False, "erro": f"ação inválida: {acao}. Use: {', '.join(ACOES_JANELA)}"}
        titulo = titulo.strip()
        if acao != "listar" and not titulo:
            return {"ok": False, "erro": "informe parte do título da janela"}
        if len(titulo) > 200 or "\x00" in titulo:
            return {"ok": False, "erro": "título inválido"}
        if platform == "win32":
            return _janela_windows(acao, titulo)
        if platform == "darwin":
            return {
                "ok": False,
                "erro": "controlar_janela não existe no macOS (exigiria Acessibilidade); "
                "para trazer um programa à frente use abrir_app",
            }
        return _janela_linux(acao, titulo)

    obj = "object"
    return [
        Tool(
            "controlar_midia",
            "Controla o som do computador: tocar_pausar, proxima, anterior, volume_mais, "
            "volume_menos ou mudo.",
            {
                "type": obj,
                "properties": {"acao": {"type": "string", "enum": list(ACOES_MIDIA)}},
                "required": ["acao"],
            },
            controlar_midia,
        ),
        Tool(
            "controlar_janela",
            "Lista, foca, minimiza, maximiza, restaura ou fecha uma janela pelo título. Fechar "
            "pode perder trabalho não salvo: pede confirmação. Os títulos são externos.",
            {
                "type": obj,
                "properties": {
                    "acao": {"type": "string", "enum": list(ACOES_JANELA)},
                    "titulo": {"type": "string", "description": "parte do título da janela"},
                },
                "required": ["acao"],
            },
            controlar_janela,
        ),
    ]
