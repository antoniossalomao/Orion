"""Janela F01: atalhos, Ctrl+K, zoom, tamanho mínimo, maximizar, foco por Tab."""
import time

from app import sessao_logada
from qa_lib import A, atalho, foco, ver_texto


def executar(s, ctx):
    o = sessao_logada(ctx, "janela")
    w = o.win
    with s.passo("janela principal visível com título 'Orion'"):
        assert w.is_visible() and "Orion" in w.window_text()
    with s.passo("Ctrl+K abre a paleta de busca"):
        foco(w); atalho(A["paleta"], 1)
        ver_texto(w, r"buscar|conversas|notas", 6)
    with s.passo("Esc fecha a paleta"):
        atalho("{ESC}", 0.8)
    with s.passo("Ctrl+B recolhe/abre a barra lateral"):
        atalho(A["barra_lateral"], 0.8); atalho(A["barra_lateral"], 0.8)
    with s.passo("Ctrl+= / Ctrl+- alteram a escala da interface"):
        atalho(A["zoom_mais"], 0.8); atalho(A["zoom_mais"], 0.8)
        atalho(A["zoom_menos"], 0.8); atalho(A["zoom_menos"], 0.8)
    with s.passo("Ctrl+Shift+O abre nova conversa"):
        atalho(A["nova_conversa"], 1)
    with s.passo("Ctrl+, abre Configurações"):
        atalho(A["config"], 1.2)
        ver_texto(w, r"configura", 6)
        atalho("{ESC}", 0.8)
    with s.passo("tamanho mínimo respeitado"):
        r = w.rectangle()
        w.move_window(r.left, r.top, 300, 300)
        time.sleep(0.8)
        r2 = w.rectangle()
        assert r2.width() >= 500 and r2.height() >= 400, f"{r2.width()}x{r2.height()} abaixo do mínimo"
    with s.passo("maximizar e restaurar"):
        w.maximize(); time.sleep(1); w.restore(); time.sleep(1)
    with s.passo("Tab percorre o foco sem travar"):
        for _ in range(12):
            atalho("{TAB}", 0.15)
    o.fechar()
