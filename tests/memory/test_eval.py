"""Critério da fase 3: perguntas de um conjunto FIXO recuperam a memória certa.

O corpus é sintético (fatos já públicos no repositório + invenções neutras). As
perguntas reais do Antônio ficam em tests/eval_pessoal.local.json (fora do git)
e rodam com `python -m orion.memory.eval`.
"""

import json

import pytest

from orion.memory import MemoryStore
from orion.memory.eval import Case, load_cases, main, run_eval

FATOS = [
    "Antônio estuda Análise e Desenvolvimento de Sistemas (ADS) na UNIMAR",
    "Antônio mora em Marília-SP",
    "O notebook atual do Antônio é um Lenovo IdeaPad Slim 3 com 8GB de RAM e Windows",
    "Antônio vai migrar para um MacBook M2 de 16GB",
    "O orçamento do projeto Orion é R$0: só free tiers e as assinaturas que ele já paga",
    "Antônio prefere explicações diretas, sem enrolação",
    "O assistente se chama Orion e tem identidade masculina",
    "A palavra de ativação da voz é Orion",
    "O bot do Telegram só responde ao ID numérico do Antônio",
    "O acesso de fora de casa é pelo Tailscale, sem expor portas",
    "Antônio trabalha principalmente com Python, TypeScript e Java",
    "O backup da memória é diário, copiando o arquivo SQLite para a nuvem",
]
NOTAS = {
    "Projetos/Orion.md": "# Orion\nA memória usa SQLite com FTS5 e sqlite-vec. A fusão dos resultados é RRF.",
    "Estudos/UML.md": "# UML\nDiagrama de classes mostra herança e associação. Diagrama de sequência mostra mensagens.",
    "Estudos/POO.md": "# POO\nEncapsulamento, herança e polimorfismo são os pilares da orientação a objetos.",
}
CASOS = [
    Case("onde o Antônio faz faculdade?", ("UNIMAR",)),
    Case("em que cidade ele mora?", ("Marília",)),
    Case("qual é o notebook dele?", ("IdeaPad",)),
    Case("quanto de RAM tem o computador?", ("8GB",)),
    Case("que máquina vai usar depois?", ("MacBook",)),
    Case("quanto posso gastar no projeto?", ("R$0",)),
    Case("como ele gosta que expliquem as coisas?", ("diretas",)),
    Case("qual a palavra que ativa a voz?", ("ativação",)),
    Case("quem pode falar com o bot do Telegram?", ("ID numérico",)),
    Case("como acesso de fora de casa?", ("Tailscale",)),
    Case("quais linguagens ele usa?", ("Python",)),
    Case("com que frequência o backup roda?", ("diário",)),
    Case("o que é RRF na memória?", ("RRF",)),
    Case("quais são os pilares de orientação a objetos?", ("Encapsulamento",)),
    Case("diagrama de sequência mostra o quê?", ("mensagens",)),
]


def montar(store: MemoryStore, tmp_path):
    for f in FATOS:
        store.add_fact(f, "seed")
    vault = tmp_path / "vault"
    for rel, texto in NOTAS.items():
        (vault / rel).parent.mkdir(parents=True, exist_ok=True)
        (vault / rel).write_text(texto, encoding="utf-8")
    store.index_vault(vault)
    return store


def test_conjunto_fixo_palavra_chave(store, tmp_path):
    rel = run_eval(montar(store, tmp_path), CASOS, k=5)
    assert rel.n == len(CASOS)
    assert rel.hit_rate >= 0.8, [c.question for c in rel.misses]
    assert rel.mrr >= 0.6


def test_vetor_nao_piora_e_recupera_os_sinonimos(store, store_vec, tmp_path):
    base = run_eval(montar(store, tmp_path / "a"), CASOS, k=5)
    hibrido = run_eval(montar(store_vec, tmp_path / "b"), CASOS, k=5)
    assert hibrido.hit_rate >= base.hit_rate
    sinonimos = [
        Case("onde ele faz faculdade?", ("UNIMAR",)),
        Case("em que cidade vive?", ("Marília",)),
    ]
    assert run_eval(store_vec, sinonimos, k=3).hit_rate == 1.0


def test_relatorio_lista_os_erros(store):
    store.add_fact("Antônio mora em Marília", "seed")
    rel = run_eval(
        store, [Case("qual a cor do carro?", ("azul",)), Case("onde mora?", ("Marília",))]
    )
    assert (rel.n, rel.hit_rate) == (2, 0.5)
    assert [c.question for c in rel.misses] == ["qual a cor do carro?"]


def test_cli_e_codigo_de_saida(tmp_path, capsys):
    db = tmp_path / "orion.db"
    s = MemoryStore(db)
    s.add_fact("Antônio mora em Marília", "seed")
    s.close()
    casos = tmp_path / "casos.json"
    casos.write_text(
        json.dumps(
            [
                {"question": "onde mora?", "expect": ["Marília"]},
                {"question": "cor do carro?", "expect": ["azul"]},
            ]
        ),
        encoding="utf-8",
    )
    assert load_cases(casos)[0].expect == ("Marília",)
    assert main([str(casos), "--db", str(db), "--min-hit-rate", "0.5"]) == 0
    assert main([str(casos), "--db", str(db), "--min-hit-rate", "0.9"]) == 1
    assert "hit@5=50.0%" in capsys.readouterr().out


@pytest.mark.parametrize("k", [1, 3])
def test_k_menor_nao_quebra(store, tmp_path, k):
    assert run_eval(montar(store, tmp_path), CASOS[:3], k=k).n == 3
