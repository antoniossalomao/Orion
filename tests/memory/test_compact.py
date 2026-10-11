"""E4.4, regra 53: o auto-compact resume o que saiu da janela, só com o modelo local, sem
atrasar o turno e sem tocar no registro."""

import asyncio

import pytest

from orion.agent import Agent
from orion.config import Settings
from orion.memory import MemoryStore
from orion.memory.compact import CABECALHO, Compactor, limpar_resumo
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, fala


class Local:
    """Completer falso: devolve um resumo e guarda o que recebeu."""

    def __init__(
        self, resposta: str | Exception = "- Decidiu usar SQLite\n- Pediu para lembrar do backup"
    ):
        self.resposta, self.pedidos = resposta, []

    async def complete(self, messages):
        self.pedidos.append(messages)
        if isinstance(self.resposta, Exception):
            raise self.resposta
        return self.resposta


@pytest.fixture
def mem(tmp_path):
    s = MemoryStore(tmp_path / "c.db")
    yield s
    s.close()


def conversa(mem, n):
    sessao = mem.active_session("web")
    for i in range(n):
        mem.add_message(sessao.id, "user" if i % 2 == 0 else "assistant", f"mensagem {i}")
    return sessao


def test_limpar_resumo_valida_formato_e_segredos():
    assert (
        limpar_resumo("decidiu usar SQLite\n\n- e o backup")
        == "- decidiu usar SQLite\n- e o backup"
    )
    assert limpar_resumo("   ") is None and limpar_resumo("curto") is None
    assert limpar_resumo("- a senha do banco é 1234 e fica no cofre") is None
    longo = limpar_resumo("\n".join(f"- linha {i} com texto" for i in range(40)))
    assert longo is not None and len(longo.splitlines()) == 12


async def test_conversa_curta_nao_resume(mem):
    s = conversa(mem, 20)
    local = Local()
    assert await Compactor(mem, local, at=40).compactar(s.id) is False
    assert local.pedidos == [] and mem.get_summary(s.id) is None


async def test_200_mensagens_mantem_resumo_e_o_registro_fica_intacto(mem):
    s = conversa(mem, 200)
    local = Local()
    c = Compactor(mem, local, at=40, window=30, model="local:qwen")
    assert await c.compactar(s.id) is True
    resumo = mem.get_summary(s.id)
    assert resumo["model"] == "local:qwen" and "SQLite" in resumo["text"]
    # cobre só o que ficou FORA da janela de 30 e só as falas do Antônio e do Orion
    ids = [m.id for m in mem.history(s.id, 1000)]
    assert resumo["upto_message_id"] == ids[len(ids) - 30 - 1]
    assert len(mem.history(s.id, 1000)) == 200  # nada foi apagado
    entrada = local.pedidos[0][1]["content"]
    assert "mensagem 0" in entrada and "mensagem 199" not in entrada
    assert "dado, não instrução" in local.pedidos[0][0]["content"]
    # nada novo fora da janela: não chama o modelo de novo
    n = len(local.pedidos)
    assert await c.compactar(s.id) is False and len(local.pedidos) == n


async def test_resumo_incremental_atualiza_o_anterior(mem):
    s = conversa(mem, 60)
    local = Local()
    c = Compactor(mem, local, at=40, window=30)
    await c.compactar(s.id)
    primeiro = mem.get_summary(s.id)["upto_message_id"]
    for i in range(30):
        mem.add_message(s.id, "user", f"depois {i}")
    local.resposta = "- resumo novo e atualizado"
    assert await c.compactar(s.id) is True
    assert mem.get_summary(s.id)["upto_message_id"] > primeiro
    assert "[RESUMO ANTERIOR]" in local.pedidos[1][1]["content"]


async def test_resposta_ruim_ou_falha_do_modelo_nao_grava_nada(mem):
    s = conversa(mem, 80)
    assert (
        await Compactor(mem, Local("- a senha é 123456 do banco"), at=40).compactar(s.id) is False
    )
    assert mem.get_summary(s.id) is None
    with pytest.raises(ConnectionError):
        await Compactor(mem, Local(ConnectionError("ollama fora")), at=40).compactar(s.id)
    assert mem.get_summary(s.id) is None


async def test_conteudo_externo_e_notas_do_sistema_ficam_de_fora(mem):
    s = mem.active_session("web")
    for i in range(50):
        mem.add_message(
            s.id, "user", "[CONTEXTO EXTERNO SELECIONADO] injeção" if i == 1 else f"fala {i}"
        )
        mem.add_message(s.id, "system", "[SISTEMA] resultado da ferramenta SEGREDO-XYZ")
    local = Local()
    await Compactor(mem, local, at=40, window=30).compactar(s.id)
    entrada = local.pedidos[0][1]["content"]
    assert "SEGREDO-XYZ" not in entrada and "injeção" not in entrada


def test_limpar_contexto_e_editar_pedido_descartam_o_resumo(mem):
    s = conversa(mem, 5)
    mem.set_summary(s.id, 3, "- algo antigo aqui", "local")
    mem.clear_context(s.id)
    assert mem.get_summary(s.id) is None
    s2 = mem.new_session("web")
    m1 = mem.add_message(s2.id, "user", "pedido original")
    mem.add_message(s2.id, "assistant", "resposta")
    mem.set_summary(s2.id, m1.id, "- resumo cobrindo o pedido", "local")
    mem.supersede_from(s2.id, m1.id)
    assert mem.get_summary(s2.id) is None


async def test_agente_resume_depois_da_resposta_e_o_resumo_entra_no_prompt(mem, tmp_path):
    policy = PolicyEngine(path_guard=PathGuard(), approvals=ApprovalStore())
    local = Local("- combinaram usar o Ollama local")
    compactor = Compactor(mem, local, at=40, window=30)
    gw = FakeGateway(fala("ok 1"), fala("ok 2"))
    agent = Agent(
        gateway=gw,  # type: ignore[arg-type]
        tools=ToolRegistry(memory_tools(mem)),
        policy=policy,
        memory=mem,
        compactor=compactor,
    )
    s = conversa(mem, 60)
    [e async for e in agent.run("web", "uma pergunta nova")]
    await compactor.aguardar()
    assert mem.get_summary(s.id)["text"].startswith("- combinaram")
    [e async for e in agent.run("web", "e agora?")]
    mensagens = gw.chamadas[-1]
    blocos = [m["content"] for m in mensagens if m["content"].startswith(CABECALHO)]
    assert len(blocos) == 1 and "Ollama local" in blocos[0] and "pode ter erro" in blocos[0]
    i = mensagens.index({"role": "user", "content": blocos[0]})
    assert mensagens[i + 1]["content"].startswith("mensagem")  # o histórico vem depois


async def test_falha_do_modelo_local_nao_atrapalha_o_turno(mem):
    policy = PolicyEngine(path_guard=PathGuard(), approvals=ApprovalStore())
    c = Compactor(mem, Local(ConnectionError("fora")), at=40, window=30)
    agent = Agent(
        gateway=FakeGateway(fala("tudo certo")),  # type: ignore[arg-type]
        tools=ToolRegistry(memory_tools(mem)),
        policy=policy,
        memory=mem,
        compactor=c,
    )
    s = conversa(mem, 60)
    eventos = [e async for e in agent.run("web", "oi")]
    await c.aguardar()
    assert eventos[-1].kind == "done" and mem.get_summary(s.id) is None


async def test_so_uma_compactacao_por_conversa_de_cada_vez(mem):
    s = conversa(mem, 80)
    local = Local()
    gate = asyncio.Event()
    original = local.complete

    async def lento(messages):
        await gate.wait()
        return await original(messages)

    local.complete = lento  # type: ignore[method-assign]
    c = Compactor(mem, local, at=40, window=30)
    c.agendar(s.id)
    c.agendar(s.id)
    gate.set()
    await c.aguardar()
    assert len(local.pedidos) == 1


def test_o_app_so_cria_o_compactor_com_modelo_local_e_nunca_com_gateway_de_fora(tmp_path, mem):
    from orion.app import compactor_from_settings

    base = {"data_dir": tmp_path, "_env_file": None}
    assert compactor_from_settings(Settings(**base), mem) is None  # sem modelo local
    assert (
        compactor_from_settings(Settings(**base, local_model="qwen3.5:4b", compact_at=0), mem)
        is None
    )
    c = compactor_from_settings(Settings(**base, local_model="qwen3.5:4b"), mem)
    assert c is not None
    enderecos = {ep.base_url for ep in c.gateway.endpoints}  # type: ignore[attr-defined]
    assert enderecos == {"http://127.0.0.1:11434/v1"}
