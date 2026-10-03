import json

import pytest

from orion.__main__ import main
from orion.migration import verify_export
from tests.memory.conftest import escrever


def test_export_completo_passa_no_ensaio_de_importacao_backup_e_restauracao(export_completo):
    rel = verify_export(export_completo, assistentes=["Orion", "Lyra"])
    assert rel.ok, rel.erros
    assert rel.tabelas["evento"] == 8 and rel.tabelas["agendamento"] == 4
    assert rel.ausentes == []  # todas as tabelas esperadas existem neste export
    assert rel.importacao and rel.importacao.mensagens == 6
    assert rel.ensaio["messages"] == 6 and rel.ensaio["edges"] == 4 and rel.ensaio["tasks"] == 2
    texto = "\n".join(rel.linhas())
    assert "RESULTADO: PRONTO" in texto and "BACKUP E RESTAURAÇÃO" in texto
    # registros inválidos do export aparecem como aviso (não derrubam, mas não passam em silêncio)
    assert any("inválido(s) em agendamento.json" in a for a in rel.avisos)


def test_tabela_que_o_legado_nunca_usou_e_so_aviso(export):
    rel = verify_export(export, assistentes=["Orion", "Lyra"])
    assert rel.ok, rel.erros
    assert {"agendamento", "tarefa", "prompt", "precedeu", "sobre", "conecta"} <= set(rel.ausentes)


def test_ator_desconhecido_e_avisado_para_nao_perder_as_respostas_do_nome_antigo(export):
    rel = verify_export(export, assistentes=["Orion"])  # "Lyra" não informado
    assert rel.ok
    assert any("lyra" in a and "--assistente" in a for a in rel.avisos)
    assert rel.importacao and rel.importacao.atores_como_system == {"lyra": 1}


def test_export_sem_os_arquivos_obrigatorios_ou_vazio_ou_corrompido_nao_esta_pronto(tmp_path):
    assert not verify_export(tmp_path / "nao-existe").ok
    pasta = tmp_path / "bk"
    pasta.mkdir()
    rel = verify_export(pasta)
    assert not rel.ok and any("evento.json não existe" in e for e in rel.erros)
    assert any("sessao.json não existe" in e for e in rel.erros)
    escrever(pasta, "evento", [])
    escrever(pasta, "sessao", [])
    assert any("vazio" in e for e in verify_export(pasta).erros)
    (pasta / "evento.json").write_text("{quebrado", encoding="utf-8")
    rel = verify_export(pasta)
    assert any("evento.json ilegível" in e for e in rel.erros)
    assert not any("evento.json não existe" in e for e in rel.erros)  # um erro só, não dois
    (pasta / "evento.json").write_text('{"nao": "lista"}', encoding="utf-8")
    assert any("lista de registros" in e for e in verify_export(pasta).erros)
    assert rel.importacao is None  # com erro no export, nem tenta o ensaio


def test_conferencia_dos_arquivos_a_copiar_do_pc_sem_vazar_valor_de_segredo(export, tmp_path):
    env = tmp_path / ".env"
    env.write_text(
        "# comentário\nGROQ_API_KEY=gsk_" + "s" * 30 + "\nTELEGRAM_BOT_TOKEN=123:abc\nVAZIA=\n",
        encoding="utf-8",
    )
    gauth = tmp_path / "google_auth"
    gauth.mkdir()
    (gauth / "token.json").write_text("{}", encoding="utf-8")
    (gauth / "vazio.json").write_text("", encoding="utf-8")
    vault = tmp_path / "vault"
    (vault / ".obsidian").mkdir(parents=True)
    (vault / ".obsidian" / "x.md").write_text("config", encoding="utf-8")
    (vault / "Nota.md").write_text("# nota", encoding="utf-8")
    rel = verify_export(
        export, assistentes=["Orion", "Lyra"], env_file=env, google_auth=gauth, vault=vault
    )
    assert rel.ok, rel.erros
    texto = "\n".join(rel.linhas())
    assert "GROQ_API_KEY" in texto and "TELEGRAM_BOT_TOKEN" in texto and "VAZIA" not in texto
    assert "gsk_" not in texto and "123:abc" not in texto  # só os nomes, nunca os valores
    assert (
        rel.arquivos["google_auth/"].startswith("1 arquivo")
        and rel.arquivos["vault"] == "1 notas .md"
    )


def test_arquivos_ausentes_ou_vazios_reprovam(export, tmp_path):
    vazio = tmp_path / "vazio"
    vazio.mkdir()
    rel = verify_export(
        export,
        assistentes=["Orion", "Lyra"],
        env_file=tmp_path / "nao-tem.env",
        google_auth=vazio,
        vault=vazio,
    )
    assert not rel.ok and len(rel.erros) == 3
    assert any(".env" in e for e in rel.erros) and any("google_auth" in e for e in rel.erros)
    assert any("vault" in e for e in rel.erros)


def test_ensaio_nao_toca_no_banco_real(export, tmp_path, monkeypatch):
    dados = tmp_path / "dados"
    monkeypatch.setenv("ORION_DATA_DIR", str(dados))
    verify_export(export, assistentes=["Orion", "Lyra"])
    assert not (dados / "orion.db").exists()


def test_cli_verify_export_codigo_de_saida_e_relatorio(
    export_completo, tmp_path, monkeypatch, capsys
):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.setenv("ORION_LOG_JSON", "false")
    assert main(["verify-export", str(export_completo), "--assistente", "Lyra"]) == 0
    saida = capsys.readouterr().out
    assert "RESULTADO: PRONTO" in saida and "evento: 8 registros" in saida
    assert main(["verify-export", str(export_completo), "--sem-ensaio"]) == 0
    assert "IMPORTAÇÃO DE ENSAIO" not in capsys.readouterr().out
    assert main(["verify-export", str(tmp_path / "nada")]) == 1
    assert "NÃO está pronto" in capsys.readouterr().out


def test_conta_inconsistente_e_pega(export, monkeypatch):
    """Se a importação deixasse registros sem contar, o relatório reprova (rede de segurança)."""
    from orion.memory import importer

    original = importer._importar_operacao

    def esquece_tarefas(store, pasta, rel):
        original(store, pasta, rel)
        rel.operacionais["lembrete"] = rel.operacionais.get("lembrete", 0) - 1

    monkeypatch.setattr(importer, "_importar_operacao", esquece_tarefas)
    rel = verify_export(export, assistentes=["Orion", "Lyra"])
    assert not rel.ok and any("lembrete: 2 no export" in e for e in rel.erros)


@pytest.mark.parametrize("n", [0, 1])
def test_resumo_do_import_avisa_ator_desconhecido(export, store, n):
    from orion.memory.importer import import_surreal_export

    rel = import_surreal_export(store, export, assistentes=["Orion", "Lyra"][: n + 1])
    assert ("ATENÇÃO" in rel.resumo()) is (n == 0)
    json.dumps(rel.atores_como_system)  # serializável (vira JSON em relatórios)
