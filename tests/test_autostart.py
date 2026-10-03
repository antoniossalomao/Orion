import plistlib
from pathlib import Path

import pytest

from orion.__main__ import main
from orion.autostart import ROTULO, detectar, instalar, render

PROJ = Path("/home/ana & co/Orion")
LOGS = Path("/dados/logs")
PY = "/venv/bin/python"


def gerar(plataforma, tmp_path=None, **kw):
    return render(plataforma, python=PY, projeto=PROJ, logs=LOGS, home=Path("/home/ana"), **kw)


def test_macos_gera_plist_valido_com_escape_e_a_pasta_do_projeto():
    a = gerar("macos")
    dados = plistlib.loads(a.conteudo.encode())  # XML válido, mesmo com "&" no caminho
    assert dados["Label"] == ROTULO
    assert dados["ProgramArguments"] == [PY, "-m", "orion", "serve"]
    assert dados["WorkingDirectory"] == str(PROJ)  # é de lá que o .env é lido
    assert dados["RunAtLoad"] is True and dados["KeepAlive"] is True
    assert dados["StandardOutPath"] == str(LOGS / "orion.out.log")
    assert a.arquivo == Path("/home/ana/Library/LaunchAgents/com.orion.assistente.plist")
    assert "launchctl bootstrap" in a.ativar and "launchctl bootout" in a.desativar


def test_linux_gera_unit_do_systemd_com_caminho_com_espaco_e_percent_escapado():
    a = render(
        "linux",
        python="/opt/meu py/python",
        projeto=Path("/srv/100%/Orion"),
        logs=LOGS,
        home=Path("/home/ana"),
    )
    assert 'ExecStart="/opt/meu py/python" -m orion serve' in a.conteudo
    # % é especificador no systemd; o caminho aparece na forma nativa do sistema (\\ no Windows)
    assert f"WorkingDirectory={str(Path('/srv/100%/Orion')).replace('%', '%%')}" in a.conteudo
    assert "Restart=on-failure" in a.conteudo and "WantedBy=default.target" in a.conteudo
    assert a.arquivo == Path("/home/ana/.config/systemd/user/orion.service")
    assert "enable --now orion.service" in a.ativar


def test_windows_gera_cmd_na_pasta_inicializar_sem_expandir_variaveis(monkeypatch):
    monkeypatch.setenv("APPDATA", "C:/Users/ana/AppData/Roaming")
    a = render(
        "windows",
        python="C:/py thon/python.exe",
        projeto=Path("C:/Orion %PATH%"),
        logs=LOGS,
        home=Path("C:/Users/ana"),
    )
    assert a.arquivo.name == "orion.cmd" and "Startup" in a.arquivo.parts
    esperado = str(Path("C:/Orion %PATH%")).replace("%", "%%")  # %PATH% literal, não expandido
    assert f'cd /d "{esperado}"' in a.conteudo and "%%PATH%%" in a.conteudo
    assert 'start "Orion" /min "C:/py thon/python.exe" -m orion serve' in a.conteudo
    assert a.conteudo.count("\r\n") == a.conteudo.count("\n")  # fim de linha do Windows


def test_plataforma_desconhecida_e_recusada():
    with pytest.raises(ValueError, match="desconhecida"):
        gerar("beos")
    assert detectar() in ("windows", "macos", "linux")


def test_instalar_nao_sobrescreve_sem_force_e_cria_a_pasta(tmp_path):
    a = gerar("linux")
    alvo = instalar(a, destino=tmp_path / "novo" / "pasta")
    assert alvo.read_text(encoding="utf-8") == a.conteudo
    with pytest.raises(FileExistsError, match="--force"):
        instalar(a, destino=tmp_path / "novo" / "pasta")
    alvo.write_text("editado", encoding="utf-8")
    instalar(a, destino=tmp_path / "novo" / "pasta", force=True)
    assert alvo.read_text(encoding="utf-8") == a.conteudo


def test_cli_so_imprime_por_padrao_e_grava_com_dir(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "dados"))
    monkeypatch.setenv("ORION_LOG_JSON", "false")
    assert main(["autostart", "--plataforma", "linux"]) == 0
    saida = capsys.readouterr().out
    assert "ExecStart=" in saida and "para ligar: systemctl --user" in saida
    assert not list(tmp_path.rglob("orion.service"))  # imprimir não grava nada

    assert main(["autostart", "--plataforma", "linux", "--dir", str(tmp_path / "out")]) == 0
    assert (tmp_path / "out" / "orion.service").is_file()
    assert "gravado:" in capsys.readouterr().out
    assert main(["autostart", "--plataforma", "linux", "--dir", str(tmp_path / "out")]) == 1
    assert "--force" in capsys.readouterr().err
    assert main(["autostart", "--plataforma", "macos", "--dir", str(tmp_path / "m")]) == 0
    assert (tmp_path / "dados" / "logs").is_dir()  # o launchd exige a pasta de logs
