"""Classes de risco por ferramenta (regra 1 do ORION_REGRAS.md).

A classe é fixa por ferramenta e declarada aqui — não inferida por regex no
texto da chamada. O argumento dá só o refinamento (comando de shell, caminho).
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum


class Risk(StrEnum):
    READ = "read"  # roda direto
    WRITE = "write"  # roda e fica no audit
    EXEC = "exec"  # shell/UI/agente: confirma, salvo leitura provada
    DESTRUCTIVE = "destructive"  # sempre confirma


@dataclass(frozen=True)
class ToolSpec:
    name: str
    risk: Risk
    cmd_arg: str | None = None  # argumento com comando de shell (EXEC)
    path_arg: str | None = None  # argumento com caminho que a ferramenta escreve
    organize: bool = False  # path_arg é uma pasta que será reorganizada
    external: bool = False  # devolve conteúdo não confiável (web, e-mail, arquivos)


def _t(name: str, risk: Risk, **kw) -> tuple[str, ToolSpec]:
    return name, ToolSpec(name, risk, **kw)


# As 55 ferramentas do legado, nos nomes em PT (contrato de function-calling).
DEFAULT_TOOLS: dict[str, ToolSpec] = dict(
    [
        # leitura
        _t("ler_arquivo", Risk.READ),
        _t("listar_arquivos", Risk.READ),
        _t("ler_clipboard", Risk.READ, external=True),
        _t("analisar_clipboard_com_ia", Risk.READ, external=True),
        _t("ler_documento", Risk.READ, external=True),
        _t("resumir_documento", Risk.READ, external=True),
        _t("transcrever_audio", Risk.READ),
        _t("traduzir_texto", Risk.READ),
        _t("consultar_git", Risk.READ),
        _t("buscar_memoria", Risk.READ),
        _t("listar_numeros", Risk.READ),
        _t("status_processo_bg", Risk.READ),
        _t("listar_processos_bg", Risk.READ),
        _t("listar_vigilancias", Risk.READ),
        _t("consultar_audit_log", Risk.READ),
        _t("checar_saude_sistema", Risk.READ),
        _t("checar_servicos_orion", Risk.READ),
        _t("capturar_tela", Risk.READ),
        _t("explicar_tela", Risk.READ),
        _t("analisar_imagem", Risk.READ),
        _t("consultar_clima", Risk.READ),
        _t("obter_topico_celular", Risk.READ),
        _t("status_enxame", Risk.READ),
        _t("listar_eventos", Risk.READ, external=True),
        # leitura de conteúdo externo (fonte de prompt injection)
        _t("pesquisar_internet", Risk.READ, external=True),
        _t("pesquisar_com_ia", Risk.READ, external=True),
        _t("buscar_url", Risk.READ, external=True),
        _t("ler_emails", Risk.READ, external=True),
        _t("ler_email", Risk.READ, external=True),
        # escrita (com log)
        _t("escrever_arquivo", Risk.WRITE, path_arg="path"),
        _t("gerar_documento", Risk.WRITE, path_arg="path"),
        _t("organizar_pasta", Risk.WRITE, path_arg="path", organize=True),
        _t("escrever_clipboard", Risk.WRITE),
        _t("salvar_memoria", Risk.WRITE),
        _t("registrar_numero", Risk.WRITE),
        _t("gerenciar_lembretes", Risk.WRITE),
        _t("gerenciar_agendamentos", Risk.WRITE),
        _t("gerenciar_tarefas", Risk.WRITE),
        _t("notificar_usuario", Risk.WRITE),
        _t("notificar_celular", Risk.WRITE),
        _t("criar_rascunho_email", Risk.WRITE),
        _t("criar_evento", Risk.WRITE),
        _t("gerar_imagem", Risk.WRITE),
        _t("abrir_app", Risk.WRITE),
        _t("controlar_midia", Risk.WRITE),
        _t("backup_memoria", Risk.WRITE),
        _t("iniciar_vigilancia_pasta", Risk.WRITE),
        _t("parar_vigilancia_pasta", Risk.WRITE),
        _t("criar_enxame", Risk.WRITE),
        _t("consolidar_enxame", Risk.WRITE),
        # execução
        _t("executar_comando", Risk.EXEC, cmd_arg="cmd"),
        _t("iniciar_processo_bg", Risk.EXEC, cmd_arg="comando"),
        _t("controlar_janela", Risk.EXEC),
        _t("navegar_web", Risk.EXEC, external=True),
        _t("consultar_especialista", Risk.EXEC),
    ]
)

# Limites do legado (chamadas, janela em segundos), mantidos.
DEFAULT_RATE_LIMITS: dict[str, tuple[int, int]] = {
    "executar_comando": (20, 300),
    "iniciar_processo_bg": (5, 300),
    "escrever_arquivo": (30, 60),
    "organizar_pasta": (3, 300),
    "consultar_especialista": (3, 600),
    "navegar_web": (10, 300),
}
