"""Painel único: o estado do Orion numa tela só (e num comando do Telegram).

Junta o que antes ficava espalhado: por onde as respostas dos modelos passaram (e se algum
endpoint está em quarentena por cota), quanto das CLIs oficiais foi usado hoje, aprovações
esperando você, o que a política decidiu nas últimas 24 h, avisos na fila, jobs, memória, canais e
servidores MCP.

Só leitura e **sem segredo**: nome de endpoint, modelo, contagens e o tipo do último erro (nunca o
corpo da resposta, a URL nem a chave); aprovações e decisões aparecem sem os argumentos (quem
quiser ver o que se aprova abre o cartão de aprovação, que mostra tudo). Os números dos modelos
são o que **o Orion viu desde que subiu**: não são a cota do provedor (quem a conhece é o
OmniRoute).
"""

from __future__ import annotations

import time
from collections import Counter
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .agent import Agent
    from .delegate import Delegator
    from .jobs import JobRunner
    from .mcp_client import McpManager
    from .memory import MemoryStore
    from .memory.ops import Operations
    from .policy import PolicyEngine

JANELA_H = 24
RECENTES = 8


class Painel:
    def __init__(
        self,
        *,
        started_at: float,
        memory: MemoryStore,
        ops: Operations,
        policy: PolicyEngine,
        agent: Agent | None = None,
        jobs: JobRunner | None = None,
        mcp: McpManager | None = None,
        delegator: Delegator | None = None,
        telegram_ativo: Callable[[], bool] = lambda: False,
        voz: Callable[[], dict[str, Any]] | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.started_at = started_at
        self.memory, self.ops, self.policy = memory, ops, policy
        self.agent, self.jobs, self.mcp, self.delegator = agent, jobs, mcp, delegator
        self.telegram_ativo = telegram_ativo
        self._voz = voz
        self._clock = clock

    def montar(self) -> dict[str, Any]:
        agora = self._clock()
        gw = getattr(self.agent, "gateway", None) if self.agent else None
        endpoints = gw.stats() if gw is not None and hasattr(gw, "stats") else []
        pendentes = self.policy.approvals.pending()
        try:
            memoria_ok = bool(self.memory.ping())
        except Exception:  # noqa: BLE001 — o painel nunca pode levantar por um componente
            memoria_ok = False
        return {
            "gerado_em": agora,
            "uptime_s": round(agora - self.started_at),
            "modelos": {"configurado": self.agent is not None, "endpoints": endpoints},
            "roteamento": {
                "ativo": bool(getattr(self.agent, "routing", False)),
                "contagem": dict(getattr(self.agent, "rotas", {})),
            },
            "clis": self.delegator.status() if self.delegator is not None else [],
            "aprovacoes": {
                "pendentes": len(pendentes),
                "itens": [
                    {
                        "id": a.id,
                        "ferramenta": a.tool,
                        "motivo": a.reason[:200],
                        "idade_s": round(max(0.0, agora - a.created_at)),
                        "expira_em_s": round(max(0.0, a.expires_at - agora)),
                    }
                    for a in pendentes[:10]
                ],
            },
            "decisoes": self._decisoes(agora),
            "avisos": {"pendentes": len(self.ops.pending_notifications(limit=1000))},
            "jobs": {
                "ativo": self.jobs is not None,
                "ultima_rodada": getattr(self.jobs, "ultima_rodada", None),
                "erros": list(getattr(self.jobs, "ultimos_erros", []))[:5],
            },
            "memoria": {"ok": memoria_ok, "vetores": self.memory.vectors_available},
            "canais": {"telegram": bool(self.telegram_ativo())},
            "voz": self._voz() if self._voz is not None else None,
            "ferramentas": len(self.agent.tools.names()) if self.agent is not None else 0,
            "mcp": dict(self.mcp.status) if self.mcp is not None else {},
        }

    def _decisoes(self, agora: float) -> dict[str, Any]:
        linhas = self.ops.audit_recent(500, desde=agora - JANELA_H * 3600)
        por_acao = Counter(str(r["action"]) for r in linhas)
        por_ferramenta = Counter(str(r["tool"]) for r in linhas)
        return {
            "janela_h": JANELA_H,
            "total": len(linhas),
            "truncado": len(linhas) >= 500,
            "por_acao": {a: por_acao.get(a, 0) for a in ("allow", "confirm", "deny")},
            "mais_usadas": [{"ferramenta": t, "n": n} for t, n in por_ferramenta.most_common(5)],
            "recentes": [
                {
                    "ts": r["ts"],
                    "ferramenta": r["tool"],
                    "acao": r["action"],
                    "risco": r["risk"],
                    "motivo": str(r["reason"])[:160],
                }
                for r in linhas[:RECENTES]
            ],
        }


def _dur(segundos: float) -> str:
    s = int(segundos)
    if s < 90:
        return f"{s}s"
    if s < 5400:
        return f"{s // 60} min"
    if s < 172800:
        return f"{s // 3600} h"
    return f"{s // 86400} d"


def texto_do_painel(p: dict[str, Any]) -> str:
    """O painel em texto puro (Telegram): uma seção por assunto, só o que importa."""
    linhas = [f"📊 Painel do Orion · no ar há {_dur(p['uptime_s'])}"]

    m = p["modelos"]
    if not m["configurado"]:
        linhas.append("\n🧠 Modelos: gateway não configurado.")
    else:
        linhas.append("\n🧠 Modelos (desde que o Orion subiu)")
        for e in m["endpoints"]:
            camada = f", camada {e['camada']}" if e.get("camada", "padrão") != "padrão" else ""
            estado = f"⛔ quarentena {_dur(e['quarentena_s'])}" if e["quarentena_s"] else "ok"
            extra = f", {e['limitada']}× cota" if e["limitada"] else ""
            erro = f" · último erro: {e['ultimo_erro']}" if e["ultimo_erro"] else ""
            quem = f"{e['nome']} ({e['modelo']}{camada})"
            linhas.append(f"• {quem}: {e['ok']} ok, {e['falhas']} falha(s){extra} · {estado}{erro}")
            if e.get("provedores"):
                servidos = ", ".join(f"{k} ×{v}" for k, v in e["provedores"].items())
                linhas.append(f"   serviu: {servidos}")

    if p["clis"]:
        linhas.append("\n💻 CLIs oficiais (hoje)")
        for c in p["clis"]:
            if not c["instalada"]:
                linhas.append(f"• {c['nome']}: não instalada")
            else:
                linhas.append(f"• {c['nome']}: {c['usadas_hoje']}/{c['limite_diario']} usadas")

    r = p.get("roteamento") or {}
    if r.get("ativo"):
        c = r.get("contagem", {})
        linhas.append(
            f"• roteamento: {c.get('rapido', 0)} rápidas, {c.get('pesado', 0)} pesadas, "
            f"{c.get('visao', 0)} com imagem"
        )

    a = p["aprovacoes"]
    linhas.append(f"\n✋ Aprovações pendentes: {a['pendentes']}")
    linhas.extend(
        f"• {i['ferramenta']} (expira em {_dur(i['expira_em_s'])})" for i in a["itens"][:5]
    )

    d = p["decisoes"]
    ac = d["por_acao"]
    linhas.append(
        f"\n🛡️ Política, últimas {d['janela_h']} h: {d['total']} decisões "
        f"({ac['allow']} liberadas, {ac['confirm']} pediram aval, {ac['deny']} negadas)"
    )
    if d["mais_usadas"]:
        linhas.append(
            "• mais usadas: " + ", ".join(f"{x['ferramenta']} ×{x['n']}" for x in d["mais_usadas"])
        )

    j = p["jobs"]
    if not j["ativo"]:
        linhas.append("\n⏱️ Jobs: desligados")
    else:
        linhas.append(
            "\n⏱️ Jobs: " + ("com erro — " + "; ".join(j["erros"]) if j["erros"] else "ok")
        )
    linhas.append(
        f"🔔 Avisos na fila: {p['avisos']['pendentes']} · 🧰 Ferramentas: {p['ferramentas']} · "
        f"💾 Memória: {'ok' if p['memoria']['ok'] else 'ERRO'}"
    )
    v = p.get("voz")
    if v and (v["clique"]["ligada"] or v["ao_vivo"]["ligada"]):
        partes = []
        if v["clique"]["ligada"]:
            fala = "com fala" if v["clique"]["fala"] else "só texto"
            partes.append(f"clique {v['clique']['turnos']} turno(s), {fala}")
        if v["ao_vivo"]["ligada"]:
            partes.append(
                f"ao vivo {v['ao_vivo']['sessoes']} sessão(ões), {v['ao_vivo']['minutos']} min"
            )
        erro = f" · falhas: {v['falhas']} (última: {v['ultimo_erro']})" if v["falhas"] else ""
        linhas.append("🎙️ Voz: " + "; ".join(partes) + erro)
    if p["mcp"]:
        linhas.append("🔌 MCP: " + ", ".join(f"{k} {v}" for k, v in p["mcp"].items()))
    return "\n".join(linhas)
