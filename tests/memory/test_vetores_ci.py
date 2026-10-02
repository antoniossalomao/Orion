import os

from orion.memory import MemoryStore


def test_vetores_disponiveis_quando_exigido(tmp_path):
    """No CI do Linux a extensão TEM de carregar; nos outros SOs falha só se exigido."""
    if os.environ.get("ORION_REQUIRE_VECTORS") == "1":
        s = MemoryStore(tmp_path / "v.db")
        assert s.vectors_available
        s.close()
