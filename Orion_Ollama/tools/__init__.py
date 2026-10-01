"""tools/__init__.py — aggregates every domain submodule's MAP/SCHEMA into
TOOLS_MAP/TOOLS_SCHEMA, re-exported for backward compatibility with the
pre-split orion_tools.py API.
"""

from . import (fs, os_tools, vision, memory, documents, web, system,
              notifications, reminders, processes, numbers, specialist,
              clipboard, git_tools, email_cal, security_tools)

_MODULES = (fs, os_tools, vision, memory, documents, web, system,
           notifications, reminders, processes, numbers, specialist,
           clipboard, git_tools, email_cal, security_tools)

TOOLS_MAP: dict = {}
TOOLS_SCHEMA: list = []
for _mod in _MODULES:
    TOOLS_MAP.update(_mod.MAP)
    TOOLS_SCHEMA.extend(_mod.SCHEMA)

# Funções internas usadas pelo proactive_loop.py e orion_seguranca.py — mesmos
# nomes do orion_tools.py monolítico, agora vivendo nos submódulos de domínio.
gerenciar_lembretes = reminders.gerenciar_lembretes
listar_numeros = numbers.listar_numeros
_marcar_numero_notificado = numbers._marcar_numero_notificado
_processar_agendamentos_devidos = reminders._processar_agendamentos_devidos
_checar_processos_bg_concluidos = processes._checar_processos_bg_concluidos
notificar_usuario = notifications.notificar_usuario
