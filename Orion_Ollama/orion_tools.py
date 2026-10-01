"""orion_tools.py — compatibility shim, do not add new code here.

All tools moved to tools/ (domain submodules) during the OOP refactor of
08/2026. This file exists only so cerebro_maestro.py, orion_agentes.py and
proactive_loop.py keep working without edits. Original monolith backed up at
_lixeira/lyra_tools_ORIGINAL_pre_split.py.
"""

from tools import (  # noqa: F401
    TOOLS_MAP, TOOLS_SCHEMA,
    gerenciar_lembretes, listar_numeros, _marcar_numero_notificado,
    _processar_agendamentos_devidos, _checar_processos_bg_concluidos,
    notificar_usuario,
)
