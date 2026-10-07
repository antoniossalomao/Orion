"""Motor de política: decide allow / confirm / deny para cada chamada de ferramenta.

Ordem: ferramenta registrada → limite de uso → risco efetivo (classe + shell/
caminho) → escalada por conteúdo externo lido (taint) → aprovação fora de banda
→ audit. Falha de audit em ação que não é leitura nega a ação (fail-closed).
"""

from __future__ import annotations

import json
import logging
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Any

from .approvals import ApprovalStore
from .audit import RateLimiter, redact
from .classes import DEFAULT_RATE_LIMITS, DEFAULT_TOOLS, Risk, ToolSpec
from .paths import PathGuard
from .shell import classify_command

log = logging.getLogger("orion.policy")


class Action(StrEnum):
    ALLOW = "allow"
    CONFIRM = "confirm"
    DENY = "deny"


@dataclass(frozen=True)
class ToolCall:
    name: str
    args: dict[str, Any] = field(default_factory=dict)


@dataclass
class Context:
    """Estado de uma sessão para a política. `tainted` vira True quando a
    sessão leu conteúdo externo (web, e-mail, documento): escrita e execução
    seguintes passam a exigir confirmação (regra 4)."""

    session_id: str
    tainted: bool = False
    allowed_tools: frozenset[str] | None = None
    authorities: tuple[str, ...] = ()
    authorized: Callable[[], bool] | None = None
    project_id: str | None = None
    share_personal: bool = False
    root: str | None = None
    project_revision: float | None = None


@dataclass(frozen=True)
class Decision:
    action: Action
    risk: Risk | None
    reason: str = ""
    approval_id: str | None = None


AuditSink = Callable[[dict[str, Any]], None]


class PolicyEngine:
    def __init__(
        self,
        *,
        path_guard: PathGuard,
        approvals: ApprovalStore | None = None,
        tools: Mapping[str, ToolSpec] | None = None,
        rate_limits: Mapping[str, tuple[int, int]] | None = None,
        audit: AuditSink | None = None,
    ) -> None:
        self.path_guard = path_guard
        self.approvals = approvals or ApprovalStore()
        self.tools = dict(tools if tools is not None else DEFAULT_TOOLS)
        self.rate = RateLimiter(rate_limits if rate_limits is not None else DEFAULT_RATE_LIMITS)
        self._audit = audit

    # ── API ────────────────────────────────────────────────────────────────
    def evaluate(self, call: ToolCall, ctx: Context, *, consume_approval: bool = True) -> Decision:
        decision = self._decide(call, ctx, consume_approval=consume_approval)
        if not self._record(call, ctx, decision):
            if decision.action is Action.ALLOW and decision.risk is not Risk.READ:
                return Decision(Action.DENY, decision.risk, "audit indisponível (fail-closed)")
        return decision

    def note_result(self, call: ToolCall, ctx: Context) -> None:
        """O orquestrador chama depois de executar: ferramenta que devolve conteúdo
        externo contamina a sessão."""
        spec = self.tools.get(call.name)
        if spec is not None and spec.external:
            ctx.tainted = True

    # ── interno ────────────────────────────────────────────────────────────
    def _decide(self, call: ToolCall, ctx: Context, *, consume_approval: bool = True) -> Decision:
        spec = self.tools.get(call.name)
        if spec is None:
            return Decision(Action.DENY, None, f"ferramenta '{call.name}' não registrada")
        if ctx.authorized is not None and not ctx.authorized():
            return Decision(Action.DENY, spec.risk, "concessão ou revisão revogada")
        if ctx.allowed_tools is not None and call.name not in ctx.allowed_tools:
            return Decision(Action.DENY, spec.risk, "ferramenta fora do escopo da skill")
        scope = f"project:{ctx.project_id}" if ctx.project_id else "personal"
        if spec.scope is not None and spec.scope != scope:
            return Decision(Action.DENY, spec.risk, "ferramenta de outro escopo")
        if ctx.project_id and spec.scope is None:
            paths = {
                "ler_arquivo": "path",
                "listar_arquivos": "path",
                "escrever_arquivo": "path",
                "delegar": "pasta",
            }
            scoped = {
                "consultar_agenda",
                "consultar_disponibilidade",
                "buscar_memoria",
                "salvar_memoria",
                "listar_fatos",
                "esquecer_fato",
                "editar_fato",
                "pesquisar_internet",
                "buscar_url",
            }
            if call.name in paths:
                path = Path(str(call.args.get(paths[call.name], "")))
                if (
                    not ctx.root
                    or not path.is_absolute()
                    or not path.resolve().is_relative_to(Path(ctx.root).resolve())
                ):
                    return Decision(Action.DENY, spec.risk, "caminho fora da raiz do projeto")
            elif call.name not in scoped:
                return Decision(Action.DENY, spec.risk, "ferramenta sem isolamento de projeto")
        limite = self.rate.check(call.name)
        if limite:
            return Decision(Action.DENY, spec.risk, limite)

        motivo = self._motivo_confirmacao(spec, call, ctx)
        if motivo is None:
            return Decision(Action.ALLOW, spec.risk)

        binding = self.binding(call.name, ctx)
        if consume_approval and self.approvals.consume(
            ctx.session_id, call.name, call.args, binding=binding
        ):
            return Decision(Action.ALLOW, spec.risk, f"aprovado fora de banda ({motivo})")
        pedido = self.approvals.request(
            ctx.session_id, call.name, call.args, motivo, binding=binding
        )
        return Decision(Action.CONFIRM, spec.risk, motivo, pedido.id)

    def binding(self, name: str, ctx: Context) -> str:
        spec = self.tools.get(name)
        if not ctx.authorities and not ctx.project_id and not (spec and spec.revision):
            return ""
        return json.dumps(
            [
                ctx.project_id,
                ctx.project_revision,
                ctx.authorities,
                spec.origin if spec else None,
                spec.revision if spec else None,
            ],
            sort_keys=True,
        )

    def _motivo_confirmacao(self, spec: ToolSpec, call: ToolCall, ctx: Context) -> str | None:
        if spec.risk is Risk.DESTRUCTIVE:
            return "ação destrutiva"
        if spec.require_confirmation:
            return "ação externa exige confirmação para esta revisão"
        motivo: str | None = None
        if spec.risk is Risk.EXEC:
            if spec.cmd_arg:
                veredito = classify_command(str(call.args.get(spec.cmd_arg, "")))
                if not veredito.read_only:
                    motivo = f"execução fora da lista de leitura segura: {veredito.reason}"
            else:
                motivo = "execução/automação (agente, navegador ou UI)"
        elif spec.read_path_arg and spec.risk is Risk.READ:
            motivo = self.path_guard.check_read(str(call.args.get(spec.read_path_arg, "")))
        elif spec.path_arg and spec.risk is Risk.WRITE:
            caminho = str(call.args.get(spec.path_arg, ""))
            motivo = (
                self.path_guard.check_organize(caminho)
                if spec.organize
                else self.path_guard.check_write(caminho)
            )
        if motivo is None and ctx.tainted and spec.risk in (Risk.WRITE, Risk.EXEC):
            motivo = (
                "a sessão leu conteúdo externo (web/e-mail/documento): possível prompt injection"
            )
        return motivo

    def _record(self, call: ToolCall, ctx: Context, d: Decision) -> bool:
        if self._audit is None:
            return True
        spec = self.tools.get(call.name)
        try:
            self._audit(
                {
                    "session_id": ctx.session_id,
                    "tool": call.name,
                    "origin": self.tools[call.name].origin if call.name in self.tools else None,
                    "revision": self.tools[call.name].revision if call.name in self.tools else None,
                    "args": redact(
                        {
                            k: "***" if spec and k in spec.masked_args else v
                            for k, v in call.args.items()
                        }
                    ),
                    "action": d.action.value,
                    "risk": d.risk.value if d.risk else None,
                    "reason": d.reason,
                    "tainted": ctx.tainted,
                }
            )
            return True
        except Exception:
            log.exception("falha ao gravar audit")
            return False
