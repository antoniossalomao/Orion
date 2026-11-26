"""Login F02: senha errada (atraso crescente), senha certa, sair da conta."""
import time

from app import Orion
from qa_lib import atalho, digitar, foco, ver_texto


def executar(s, ctx):
    o = Orion(ctx, "login")
    with s.passo("abre o app na tela de login"):
        o.abrir()
        ver_texto(o.win, r"entrar|senha|login", 20)
    tempos = []
    for n in range(1, 4):
        with s.passo(f"senha errada #{n} é recusada"):
            foco(o.win)
            atalho("^a", 0.2)
            digitar("qa-admin")
            atalho("{TAB}", 0.2)
            digitar("senha-errada-qa")
            t0 = time.time()
            atalho("{ENTER}", 0.5)
            ver_texto(o.win, r"incorret|inválid|errad|aguarde|tente", 15)
            tempos.append(time.time() - t0)
    with s.passo("atraso cresce a cada tentativa errada"):
        assert tempos[-1] >= tempos[0], f"atrasos {['%.1fs' % t for t in tempos]}"
    with s.passo("senha certa entra"):
        time.sleep(8)  # espera o bloqueio da última tentativa
        o.login()
        ver_texto(o.win, r"orion", 15)
    with s.passo("sair da conta volta ao login"):
        atalho(r"^{VK_OEM_COMMA}", 1.5)  # Configurações
        from qa_lib import clicar_texto

        clicar_texto(o.win, r"sair da conta|sair", 8)
        ver_texto(o.win, r"entrar|senha|login", 10)
    o.fechar()
