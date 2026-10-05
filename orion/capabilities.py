"""Contrato público de capacidades; detalhes da instalação ficam em rota autenticada."""

from typing import Literal

from pydantic import BaseModel

from . import __version__

# Recursos de interface que ainda não têm endpoint no backend novo.
PENDING_FEATURES = (
    "memory_graph",
    "memory_categories",
    "metrics",
    "stats",
    "stats_history",
    "integrations",
    "upload",
    "tts",
    "voice",
    "model_selection",
    "plugins",
    "skills",
    "mcp",
)


class Capabilities(BaseModel):
    """Flags significam recurso utilizável nesta instalação, não apenas rota presente.

    `model=ready` significa agente configurado, sem fazer uma chamada paga ou remota
    para atestar a saúde do provedor. Não há caminhos, contas, ferramentas ou segredos
    no contrato público.
    """

    contract_version: Literal[1] = 1
    backend: Literal["orion"] = "orion"
    app_version: str = __version__
    api: Literal["online"] = "online"
    model: Literal["ready", "unavailable"]
    auth_required: bool = True
    features: dict[str, bool]
    unavailable: dict[str, str]


def describe(*, agent_ready: bool, admin_configured: bool) -> Capabilities:
    features: dict[str, bool] = dict.fromkeys(PENDING_FEATURES, False)
    features.update(
        chat=agent_ready and admin_configured,
        sessions=admin_configured,
        session_management=admin_configured,
        session_search=admin_configured,
        history=admin_configured,
        history_clear=admin_configured,
        export=admin_configured,
        approvals=admin_configured,
        notifications=admin_configured,
    )
    unavailable: dict[str, str] = dict.fromkeys(PENDING_FEATURES, "not_implemented")
    if not features["chat"]:
        unavailable["chat"] = (
            "auth_not_configured" if not admin_configured else "gateway_not_configured"
        )
    if not admin_configured:
        unavailable.update(
            sessions="auth_not_configured",
            session_management="auth_not_configured",
            session_search="auth_not_configured",
            history="auth_not_configured",
            history_clear="auth_not_configured",
            export="auth_not_configured",
            approvals="auth_not_configured",
            notifications="auth_not_configured",
        )
    return Capabilities(
        model="ready" if agent_ready else "unavailable",
        features=features,
        unavailable=unavailable,
    )
