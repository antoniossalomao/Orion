"""Projetos F08 e Rotinas: abrir telas, criar e remover item de teste."""
from app import sessao_logada
from qa_lib import atalho, clicar_texto, digitar, foco, ver_texto


def executar(s, ctx):
    o = sessao_logada(ctx, "projetos")
    w = o.win
    with s.passo("tela Projetos abre"):
        foco(w); clicar_texto(w, r"^projetos", 8); ver_texto(w, r"projetos", 6)
    with s.passo("criar projeto de teste"):
        clicar_texto(w, r"novo projeto|criar|\+", 6)
        digitar("Projeto QA"); atalho("{ENTER}", 1.5)
        ver_texto(w, r"Projeto QA", 6)
    with s.passo("remover projeto de teste"):
        clicar_texto(w, r"apagar|excluir|remover", 6)
        clicar_texto(w, r"^(confirmar|apagar|excluir|sim)", 4)
    with s.passo("tela Rotinas abre"):
        clicar_texto(w, r"^rotinas", 8); ver_texto(w, r"rotinas", 6)
    with s.passo("criar rotina de teste"):
        clicar_texto(w, r"nova rotina|criar|\+", 6)
        digitar("Rotina QA"); atalho("{ENTER}", 1.5)
        ver_texto(w, r"Rotina QA", 6)
    with s.passo("remover rotina de teste"):
        clicar_texto(w, r"apagar|excluir|remover", 6)
        clicar_texto(w, r"^(confirmar|apagar|excluir|sim)", 4)
    o.fechar()
