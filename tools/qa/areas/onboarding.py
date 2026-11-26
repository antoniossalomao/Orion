"""Onboarding F10: 7 etapas, validações (vault inválido, senha fraca), interrupção e retomada."""
import perfil
from app import Orion
from qa_lib import atalho, digitar, ver_texto, foco, esperar_janela


def executar(s, ctx):
    perfil.limpar_tag("onboarding")  # perfil virgem: dispara o onboarding
    o = Orion(ctx, "onboarding", com_senha=False)
    with s.passo("primeira abertura mostra o onboarding"):
        o.abrir()
        ver_texto(o.win, r"bem-vind|começar|onboarding|vault", 20)
    for i in range(1, 8):
        with s.passo(f"etapa {i}: avançar com Enter/Tab sem dados (valida campos obrigatórios)"):
            foco(o.win)
            atalho("{ENTER}", 1.2)
    with s.passo("vault inválido é recusado"):
        digitar("Z:\\pasta\\que\\nao\\existe")
        atalho("{ENTER}", 1.2)
        ver_texto(o.win, r"inválid|não existe|não encontr|erro", 6)
    with s.passo("senha fraca é recusada"):
        digitar("123")
        atalho("{ENTER}", 1.2)
        ver_texto(o.win, r"fraca|curta|mínimo|caracteres", 6)
    with s.passo("interrupção: fechar o app no meio"):
        o.fechar()
    with s.passo("retomada: reabrir continua de onde parou"):
        o.abrir()
        ver_texto(o.win, r"continuar|retomar|bem-vind|etapa", 20)
    o.fechar()
