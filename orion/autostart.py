"""Início automático do Orion no login (fase 5: "autostart por SO").

Gera o arquivo certo para cada sistema; **não executa** nada do sistema (nem `launchctl`,
nem `systemctl`, nem `schtasks`): imprime o comando de ativação para o Antônio rodar.
O serviço sobe com o mesmo Python que roda este comando e com a pasta do projeto como
diretório de trabalho (é de lá que o `.env` é lido).
"""

from __future__ import annotations

import os
import sys
from dataclasses import dataclass
from pathlib import Path
from xml.sax.saxutils import escape

PLATAFORMAS = ("windows", "macos", "linux")
ROTULO = "com.orion.assistente"


@dataclass(frozen=True)
class Autostart:
    plataforma: str
    arquivo: Path  # onde o sistema procura
    conteudo: str
    ativar: str  # comando (ou passo) para ligar
    desativar: str


def detectar() -> str:
    if sys.platform.startswith("win"):
        return "windows"
    if sys.platform == "darwin":
        return "macos"
    return "linux"


def _cmd(texto: str) -> str:
    return texto.replace("%", "%%")  # %VAR% é expandido pelo cmd.exe até dentro de aspas


def _unit(texto: str) -> str:
    return texto.replace("%", "%%")  # especificadores do systemd


def render(
    plataforma: str,
    *,
    python: str | None = None,
    projeto: Path,
    logs: Path,
    home: Path | None = None,
) -> Autostart:
    py = python or sys.executable
    home = home or Path.home()
    if plataforma == "windows":
        pasta = (
            Path(os.environ.get("APPDATA", str(home / "AppData" / "Roaming")))
            / "Microsoft"
            / "Windows"
            / "Start Menu"
            / "Programs"
            / "Startup"
        )
        arquivo = pasta / "orion.cmd"
        conteudo = (
            "@echo off\r\n"
            "rem Gerado por `orion autostart`. Apague este arquivo para desligar o inicio.\r\n"
            f'cd /d "{_cmd(str(projeto))}"\r\n'
            f'start "Orion" /min "{_cmd(py)}" -m orion serve\r\n'
        )
        return Autostart(
            plataforma,
            arquivo,
            conteudo,
            "Sai da sessão e entra de novo, ou rode o arquivo uma vez para testar.",
            f'del "{arquivo}"',
        )
    if plataforma == "macos":
        arquivo = home / "Library" / "LaunchAgents" / f"{ROTULO}.plist"
        args = "".join(f"\n    <string>{escape(a)}</string>" for a in (py, "-m", "orion", "serve"))
        conteudo = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{ROTULO}</string>
  <key>ProgramArguments</key>
  <array>{args}
  </array>
  <key>WorkingDirectory</key><string>{escape(str(projeto))}</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{escape(str(logs / "orion.out.log"))}</string>
  <key>StandardErrorPath</key><string>{escape(str(logs / "orion.err.log"))}</string>
</dict>
</plist>
"""
        return Autostart(
            plataforma,
            arquivo,
            conteudo,
            f'launchctl bootstrap "gui/$(id -u)" "{arquivo}"',
            f'launchctl bootout "gui/$(id -u)" "{arquivo}" && rm "{arquivo}"',
        )
    if plataforma == "linux":
        arquivo = home / ".config" / "systemd" / "user" / "orion.service"
        conteudo = f"""[Unit]
Description=Orion (assistente pessoal)
After=network-online.target

[Service]
Type=simple
WorkingDirectory={_unit(str(projeto))}
ExecStart="{_unit(py)}" -m orion serve
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"""
        return Autostart(
            plataforma,
            arquivo,
            conteudo,
            "systemctl --user daemon-reload && systemctl --user enable --now orion.service",
            f'systemctl --user disable --now orion.service && rm "{arquivo}"',
        )
    raise ValueError(f"plataforma desconhecida: {plataforma}. Use: {', '.join(PLATAFORMAS)}")


def instalar(a: Autostart, *, destino: Path | None = None, force: bool = False) -> Path:
    """Grava o arquivo (em `destino`, se dado). Não sobrescreve sem `force`."""
    alvo = destino / a.arquivo.name if destino else a.arquivo
    if alvo.exists() and not force:
        raise FileExistsError(f"{alvo} já existe (use --force para substituir)")
    alvo.parent.mkdir(parents=True, exist_ok=True)
    alvo.write_text(a.conteudo, encoding="utf-8", newline="")
    return alvo
