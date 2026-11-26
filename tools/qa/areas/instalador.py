"""Instalador: --preview, --silent, atalhos, Apps e Recursos, desinstalar. UAC não é automatizável."""
import subprocess
import time
from pathlib import Path

from qa_lib import Pulo, e_admin, encerrar, esperar_janela, iniciar, foco, atalho

CHAVE = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Orion"
INSTALACAO = Path(r"C:\Program Files\Orion")


def _reg() -> bool:
    return subprocess.run(["reg", "query", CHAVE], capture_output=True).returncode == 0  # noqa: S603,S607


def executar(s, ctx):
    setup = ctx.get("setup")
    if not setup:
        with s.passo("instalador informado"):
            raise Pulo("passe --setup <Orion Setup.exe>")
        return
    p = None
    with s.passo("--preview abre a janela do instalador (sem instalar)"):
        p = iniciar([setup, "--preview"], ctx["env"])
        foco(esperar_janela(r"Orion", 20))
    with s.passo("--preview fecha com Alt+F4 sem alterar o sistema"):
        atalho("%{F4}", 1.5)
        encerrar(p)
        assert not _reg() or ctx.get("ja_instalado"), "preview gravou chave de desinstalação"
    with s.passo("UAC do instalação real"):
        if not e_admin():
            raise Pulo("UAC usa a área segura do Windows: use desktop use ou rode o runner como Administrador")
    with s.passo("--silent instala"):
        if not e_admin():
            raise Pulo("exige Administrador")
        r = subprocess.run([setup, "--silent"], env=ctx["env"], timeout=600)  # noqa: S603
        assert r.returncode == 0, f"código {r.returncode}"
        time.sleep(2)
        assert (INSTALACAO / "orion-app.exe").is_file(), "orion-app.exe ausente"
    with s.passo("registro em Aplicativos e Recursos"):
        assert _reg(), "chave Uninstall\\Orion ausente"
    with s.passo("atalho do Menu Iniciar"):
        import os

        lnk = Path(os.environ["APPDATA"]) / r"Microsoft\Windows\Start Menu\Programs\Orion.lnk"
        assert lnk.is_file(), f"{lnk} ausente"
    with s.passo("desinstalar --silent sem lixo"):
        if not e_admin():
            raise Pulo("exige Administrador")
        r = subprocess.run([setup, "--uninstall", str(INSTALACAO), "--silent"], env=ctx["env"], timeout=300)  # noqa: S603
        assert r.returncode == 0, f"código {r.returncode}"
        time.sleep(3)
        assert not _reg(), "chave Uninstall ficou"
        assert not (INSTALACAO / "orion-app.exe").exists(), "binário ficou"
