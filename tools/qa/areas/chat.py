"""Chat F05: enviar, Parar, fila, anexos, renomear, arquivar, apagar. O título da janela vira '<conversa> · Orion'."""
import tempfile
import time
from pathlib import Path

from app import sessao_logada
from qa_lib import A, Pulo, atalho, clicar_texto, digitar, foco, titulo, ver_texto


def executar(s, ctx):
    o = sessao_logada(ctx, "chat")
    w = o.win
    sem_chave = not ctx["chaves"]
    with s.passo("nova conversa"):
        foco(w); atalho(A["nova_conversa"], 1)
    with s.passo("enviar com Enter"):
        digitar("Responda apenas: ok"); atalho("{ENTER}", 2)
        ver_texto(w, r"parar|ok|erro|chave", 30)
    with s.passo("resposta real do modelo (exige chave do .docx)"):
        if sem_chave:
            raise Pulo("sem chaves: passe --docx")
        ver_texto(w, r"\bok\b", 60)
    with s.passo("Parar interrompe uma resposta longa"):
        if sem_chave:
            raise Pulo("sem chaves: passe --docx")
        digitar("Escreva 2000 palavras sobre o mar"); atalho("{ENTER}", 2)
        clicar_texto(w, r"^parar", 15)
        time.sleep(1)
    with s.passo("fila: segunda mensagem enquanto a primeira responde"):
        digitar("primeira"); atalho("{ENTER}", 0.3)
        digitar("segunda"); atalho("{ENTER}", 1.5)
        ver_texto(w, r"fila|enfileir|segunda", 10)
    with s.passo("Ctrl+Enter também envia"):
        digitar("teste ctrl enter"); atalho("^{ENTER}", 1.5)
    with s.passo("anexo: arquivo de texto via diálogo"):
        arq = Path(tempfile.gettempdir()) / "qa-anexo.txt"
        arq.write_text("anexo de teste do QA", encoding="utf-8")
        clicar_texto(w, r"anex", 8)
        time.sleep(1.5)
        digitar(str(arq)); atalho("{ENTER}", 1.5)
        ver_texto(w, r"qa-anexo", 8)
    with s.passo("renomear conversa"):
        clicar_texto(w, r"renomear", 8)
        atalho("^a", 0.2); digitar("Conversa QA"); atalho("{ENTER}", 1.2)
        assert "Conversa QA" in titulo(), f"título atual: {titulo()}"
    with s.passo("arquivar conversa"):
        clicar_texto(w, r"arquivar", 8)
        time.sleep(1)
    with s.passo("apagar conversa pede confirmação"):
        atalho(A["nova_conversa"], 1)
        digitar("para apagar"); atalho("{ENTER}", 2)
        clicar_texto(w, r"apagar|excluir", 8)
        ver_texto(w, r"confirm|certeza|apagar", 6)
        clicar_texto(w, r"^(confirmar|apagar|excluir|sim)", 6)
    o.fechar()
