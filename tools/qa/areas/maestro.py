"""Maestro: abre o editor de nós, cria nó, liga, zoom."""
from app import sessao_logada
from qa_lib import atalho, clicar_texto, foco, ver_texto


def executar(s, ctx):
    o = sessao_logada(ctx, "maestro")
    w = o.win
    with s.passo("abre o Maestro"):
        foco(w); clicar_texto(w, r"^maestro", 8); ver_texto(w, r"maestro", 6)
    with s.passo("adiciona um nó"):
        clicar_texto(w, r"novo nó|adicionar|\+", 6)
    with s.passo("adiciona segundo nó"):
        clicar_texto(w, r"novo nó|adicionar|\+", 6)
    with s.passo("liga dois nós arrastando"):
        r = w.rectangle(); cx, cy = (r.left + r.right) // 2, (r.top + r.bottom) // 2
        from pywinauto import mouse
        mouse.press(coords=(cx - 150, cy)); mouse.release(coords=(cx + 150, cy))
    with s.passo("zoom com Ctrl+roda"):
        from pywinauto import mouse
        mouse.scroll(coords=(cx, cy), wheel_dist=3); mouse.scroll(coords=(cx, cy), wheel_dist=-3)
    with s.passo("Esc sai do Maestro"):
        atalho("{ESC}", 1)
    o.fechar()
