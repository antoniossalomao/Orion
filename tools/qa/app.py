"""Ciclo de vida do Orion de teste: servidor + app visível, login, encerramento."""
import socket
import time

import perfil
from qa_lib import A, atalho, digitar, encerrar, esperar_janela, foco, iniciar, janela_existe


class Orion:
    def __init__(self, ctx, tag="geral", com_senha=True):
        self.ctx, self.tag, self.com_senha = ctx, tag, com_senha
        self.env = perfil.ambiente(ctx["chaves"], tag)
        self.srv = self.app = None

    def abrir(self):
        if self.com_senha:
            perfil.definir_senha(self.env, self.ctx["orion"])
        self.srv = iniciar([*self.ctx["orion"], "serve"], self.env)
        fim = time.time() + 60
        while time.time() < fim:
            with socket.socket() as sk:
                sk.settimeout(1)
                if sk.connect_ex(("127.0.0.1", int(perfil.PORTA))) == 0:
                    break
            time.sleep(1)
        else:
            raise TimeoutError("orion serve não abriu a porta de teste")
        self.app = iniciar(self.ctx["app"], self.env)
        self.win = esperar_janela(r"Orion", 60)
        foco(self.win)
        return self.win

    def login(self):
        a = perfil.admin()
        foco(self.win)
        atalho("^a", 0.2)
        digitar(a["usuario"])
        atalho("{TAB}", 0.2)
        digitar(a["senha"])
        atalho("{ENTER}", 2.5)

    def fechar(self):
        encerrar(self.app)
        encerrar(self.srv)

    def janela(self):
        self.win = esperar_janela(r"Orion", 10)
        foco(self.win)
        return self.win


def sessao_logada(ctx, tag="geral") -> Orion:
    o = Orion(ctx, tag)
    o.abrir()
    o.login()
    return o


__all__ = ["Orion", "sessao_logada", "A", "janela_existe"]
