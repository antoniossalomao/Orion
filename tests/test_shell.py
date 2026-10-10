"""Casca desktop: a ponte JS expõe só o necessário e recusa links fora de http(s)/mailto."""

from orion.shell import ShellApi, abrir_janela


class _Janela:
    def __init__(self):
        self.chamadas = []

    def minimize(self):
        self.chamadas.append("minimize")

    def maximize(self):
        self.chamadas.append("maximize")

    def restore(self):
        self.chamadas.append("restore")

    def destroy(self):
        self.chamadas.append("destroy")


def test_controles_da_janela():
    api = ShellApi()
    api.janela = _Janela()
    api.minimize_app()
    api.toggle_maximize()
    api.toggle_maximize()
    api.close_app()
    assert api.janela.chamadas == ["minimize", "maximize", "restore", "destroy"]


def test_sem_janela_nao_quebra():
    api = ShellApi()
    api.minimize_app()
    api.toggle_maximize()
    api.close_app()


def test_open_external_recusa_o_que_nao_e_link(monkeypatch):
    abertos = []
    monkeypatch.setattr("webbrowser.open", lambda u: abertos.append(u) or True)
    api = ShellApi()
    assert api.open_external("https://exemplo.com")
    assert api.open_external("mailto:a@b.c")
    assert not api.open_external("file:///etc/passwd")
    assert not api.open_external("javascript:alert(1)")
    assert not api.open_external(None)  # type: ignore[arg-type]
    assert abertos == ["https://exemplo.com", "mailto:a@b.c"]


def test_sem_pywebview_devolve_false(monkeypatch):
    import builtins

    real = builtins.__import__

    def sem_webview(nome, *a, **k):
        if nome == "webview":
            raise ImportError
        return real(nome, *a, **k)

    monkeypatch.setattr(builtins, "__import__", sem_webview)
    assert abrir_janela("http://127.0.0.1:1/ui/") is False
