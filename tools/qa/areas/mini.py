"""Mini barra / Island: atalho global Ctrl+Alt+Espaço traz e esconde."""
from app import sessao_logada
from qa_lib import A, atalho, janela_existe


def executar(s, ctx):
    o = sessao_logada(ctx, "mini")
    with s.passo("Ctrl+Alt+Espaço abre a mini barra"):
        atalho(A["mini"], 1.5)
        assert janela_existe(r"Orion Island|Orion"), "mini barra não apareceu"
    with s.passo("Ctrl+Alt+Espaço de novo recolhe"):
        atalho(A["mini"], 1.5)
    with s.passo("Ctrl+Alt+O abre a captura rápida"):
        atalho(A["captura"], 1.5)
        atalho("{ESC}", 0.8)
    o.fechar()
