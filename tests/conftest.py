import pytest


@pytest.fixture(autouse=True)
def _pbkdf2_barato(monkeypatch):
    """O custo do PBKDF2 (600 mil iterações) é de propósito e só atrapalha o tempo dos testes."""
    monkeypatch.setattr("orion.auth.ITERACOES", 1000)
