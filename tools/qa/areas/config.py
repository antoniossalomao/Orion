"""Configurações F07: abre, navega pelas seções e volta. Não abre campos de chave (evita segredo em print)."""
from app import sessao_logada
from qa_lib import A, atalho, clicar_texto, foco, ver_texto

SECOES = ["Geral", "Modelos", "Segurança", "Atalhos", "Vault", "Voz", "Conectores", "Aparência"]


def executar(s, ctx):
    o = sessao_logada(ctx, "config")
    w = o.win
    with s.passo("Ctrl+, abre Configurações"):
        foco(w); atalho(A["config"], 1.2); ver_texto(w, r"configura", 6)
    for nome in SECOES:
        with s.passo(f"seção {nome}"):
            clicar_texto(w, rf"^{nome}", 4)
    with s.passo("Atalhos lista Ctrl+K"):
        clicar_texto(w, r"^atalhos", 4); ver_texto(w, r"Ctrl.*K", 4)
    with s.passo("Esc volta ao chat"):
        atalho("{ESC}", 1)
    o.fechar()
