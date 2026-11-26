"""Bandeja do sistema: ícone presente, menu, fechar janela mantém o app na bandeja."""
from app import sessao_logada
from qa_lib import Pulo, atalho, desktop, foco, janela_existe


def _icone_bandeja():
    area = desktop().window(class_name="Shell_TrayWnd")
    for b in area.descendants(control_type="Button"):
        if "orion" in (b.window_text() or "").lower():
            return b
    raise LookupError("ícone do Orion não achado na bandeja (pode estar em ícones ocultos)")


def executar(s, ctx):
    o = sessao_logada(ctx, "bandeja")
    with s.passo("ícone do Orion na bandeja"):
        _icone_bandeja()
    with s.passo("clique direito abre o menu"):
        _icone_bandeja().right_click_input()
    with s.passo("Esc fecha o menu"):
        atalho("{ESC}", 0.8)
    with s.passo("Alt+F4 esconde a janela mas o app segue vivo"):
        foco(o.win); atalho("%{F4}", 1.5)
        assert o.app.poll() is None, "app encerrou em vez de ir para a bandeja"
    with s.passo("clique no ícone restaura a janela"):
        _icone_bandeja().click_input()
        assert janela_existe(r"Orion"), "janela não voltou"
    o.fechar()
