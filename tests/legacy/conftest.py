"""Carrega módulos do legado (`Orion_Ollama/`) sem Qdrant/SurrealDB no ar.

O path e o stub entram no import do conftest (antes da coleta), porque os
módulos de teste importam o legado no topo do arquivo.
"""

import sys
import types
from pathlib import Path

LEGADO = Path(__file__).parents[2] / "Orion_Ollama"

if str(LEGADO) not in sys.path:
    sys.path.insert(0, str(LEGADO))
# surreal_client abre conexão no import; os testes só exercitam lógica pura.
sys.modules.setdefault("surreal_client", types.SimpleNamespace(surreal=None))
