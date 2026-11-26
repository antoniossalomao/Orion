"""Base dos testes de usuário do Orion (Windows, janela visível, UI Automation via pywinauto).

Regras: nenhum segredo em log/print/JSON (tudo passa por `mascarar`); perfil isolado; um print
numerado por passo em qa-prints/<data>/<area>/NN-passo.png.
"""
from __future__ import annotations

import contextlib
import json
import os
import re
import subprocess
import sys
import time
import traceback
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
RUST = RAIZ / "Orion_Rust"
SEGREDOS: set[str] = set()  # valores sensíveis só em memória


def registrar_segredo(valor: str) -> None:
    if valor and len(valor) >= 6:
        SEGREDOS.add(valor)


def mascarar(texto: str) -> str:
    for s in sorted(SEGREDOS, key=len, reverse=True):
        texto = texto.replace(s, "***")
    return texto


def slug(texto: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", texto.lower()).strip("-")[:50] or "passo"


@dataclass
class Passo:
    n: int
    nome: str
    status: str  # passou | falhou | pulou
    detalhe: str = ""
    print: str = ""
    ms: int = 0


@dataclass
class Sessao:
    area: str
    pasta: Path
    passos: list[Passo] = field(default_factory=list)
    contexto: dict = field(default_factory=dict)

    # --- captura ---
    def print_tela(self, nome: str) -> str:
        try:
            from PIL import ImageGrab

            self.pasta.mkdir(parents=True, exist_ok=True)
            caminho = self.pasta / f"{len(self.passos) + 1:02d}-{slug(nome)}.png"
            ImageGrab.grab(all_screens=True).save(caminho)
            return str(caminho)
        except Exception as e:  # noqa: BLE001 - captura nunca derruba o teste
            return f"(sem print: {mascarar(str(e))})"

    @contextlib.contextmanager
    def passo(self, nome: str):
        """Executa um passo; sempre tira print; registra passou/falhou. Não propaga a exceção."""
        t0 = time.time()
        status, detalhe = "passou", ""
        try:
            yield
        except Pulo as e:
            status, detalhe = "pulou", mascarar(str(e))
        except Exception as e:  # noqa: BLE001
            status = "falhou"
            detalhe = mascarar(f"{type(e).__name__}: {e}")
            self.log(mascarar(traceback.format_exc(limit=3)))
        time.sleep(0.4)  # deixa a UI assentar antes do print
        p = Passo(len(self.passos) + 1, nome, status, detalhe, self.print_tela(nome), int((time.time() - t0) * 1000))
        self.passos.append(p)
        self.log(f"[{p.n:02d}] {status.upper():7} {nome} {detalhe}")

    def log(self, msg: str) -> None:
        self.pasta.mkdir(parents=True, exist_ok=True)
        with (self.pasta / "log.txt").open("a", encoding="utf-8") as f:
            f.write(f"{datetime.now():%H:%M:%S} {mascarar(msg)}\n")
        print(mascarar(msg), file=sys.stderr, flush=True)

    def resultado(self) -> dict:
        falhas = [p for p in self.passos if p.status == "falhou"]
        return {
            "area": self.area,
            "total": len(self.passos),
            "passou": sum(p.status == "passou" for p in self.passos),
            "falhou": len(falhas),
            "pulou": sum(p.status == "pulou" for p in self.passos),
            "falhas": [{"passo": p.n, "nome": p.nome, "erro": p.detalhe, "print": p.print} for p in falhas],
            "puladas": [{"passo": p.n, "nome": p.nome, "motivo": p.detalhe} for p in self.passos if p.status == "pulou"],
            "log": str(self.pasta / "log.txt"),
        }


class Pulo(Exception):
    """Passo não aplicável/ não automatizável aqui (ex.: UAC na área segura do Windows)."""


# --- UI Automation (importado sob demanda: lint/CI em Linux não precisa do pywinauto) ---
def desktop():
    from pywinauto import Desktop

    return Desktop(backend="uia")


def esperar_janela(titulo_re: str, timeout: float = 30):
    d = desktop()
    fim = time.time() + timeout
    while time.time() < fim:
        for w in d.windows(title_re=titulo_re, visible_only=True):
            return w
        time.sleep(0.5)
    raise TimeoutError(f"janela /{titulo_re}/ não apareceu em {timeout:.0f}s")


def janela_existe(titulo_re: str) -> bool:
    return bool(desktop().windows(title_re=titulo_re, visible_only=True))


def foco(janela) -> None:
    janela.set_focus()
    time.sleep(0.3)


def atalho(teclas: str, pausa: float = 0.6) -> None:
    """Sintaxe pywinauto: ^k = Ctrl+K, +{TAB}, %{F4} = Alt+F4. Ver `A` abaixo para os do Orion."""
    from pywinauto.keyboard import send_keys

    send_keys(teclas, pause=0.03)
    time.sleep(pausa)


def digitar(texto: str, pausa: float = 0.4) -> None:
    from pywinauto.keyboard import send_keys

    send_keys(texto.replace(" ", "{SPACE}").replace("(", "{(}").replace(")", "{)}").replace("^", "{^}").replace("%", "{%}").replace("+", "{+}"), pause=0.01)
    time.sleep(pausa)


# Atalhos do Orion (crates/orion-app/src/keys.rs)
A = {
    "paleta": "^k",
    "nova_conversa": "^+o",
    "barra_lateral": "^b",
    "config": "^{VK_OEM_COMMA}",
    "mini": "^%{SPACE}",
    "captura": "^%o",
    "isso": "^%i",
    "panico": "^%p",
    "zoom_mais": "^{VK_OEM_PLUS}",
    "zoom_menos": "^{VK_OEM_MINUS}",
}


def clicar_texto(janela, texto_re: str, timeout: float = 8) -> None:
    """Clica no 1º controle UIA cujo nome casa. iced expõe pouca árvore: se faltar, o passo falha com print."""
    fim = time.time() + timeout
    while time.time() < fim:
        for c in janela.descendants():
            try:
                if re.search(texto_re, c.window_text() or "", re.I) and c.is_visible():
                    c.click_input()
                    time.sleep(0.5)
                    return
            except Exception:  # noqa: BLE001
                continue
        time.sleep(0.5)
    raise LookupError(f"controle /{texto_re}/ não encontrado na UI Automation")


def ver_texto(janela, texto_re: str, timeout: float = 8) -> None:
    fim = time.time() + timeout
    while time.time() < fim:
        if re.search(texto_re, " ".join((c.window_text() or "") for c in janela.descendants()), re.I) or re.search(texto_re, janela.window_text() or "", re.I):
            return
        time.sleep(0.5)
    raise AssertionError(f"texto /{texto_re}/ não visível")


def titulo(janela_re: str = r"Orion") -> str:
    return esperar_janela(janela_re, 5).window_text()


# --- processos ---
def iniciar(cmd: list[str], env: dict, cwd: Path | None = None) -> subprocess.Popen:
    return subprocess.Popen(cmd, env=env, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)  # noqa: S603


def encerrar(p: subprocess.Popen | None) -> None:
    if p and p.poll() is None:
        p.terminate()
        with contextlib.suppress(Exception):
            p.wait(5)
        if p.poll() is None:
            p.kill()


def e_admin() -> bool:
    try:
        import ctypes

        return bool(ctypes.windll.shell32.IsUserAnAdmin())  # type: ignore[attr-defined]
    except Exception:  # noqa: BLE001
        return False


def json_out(obj: dict) -> str:
    return mascarar(json.dumps(obj, ensure_ascii=False, indent=2))


def ambiente_base() -> dict:
    return dict(os.environ)
