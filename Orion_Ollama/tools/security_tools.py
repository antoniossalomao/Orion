"""tools/security_tools.py — Security tools: audit-log queries."""

def consultar_audit_log(limite: int = 50, tool_filtro: str = "", apenas_bloqueados: bool = False) -> dict:
    """
    Consulta o audit log imutável da Lyra — registro de todas as ferramentas
    chamadas, com timestamp, args, resultado e se foram bloqueadas por rate limit.
    Útil pra o usuário auditar o que a Lyra fez em segundo plano.
    """
    try:
        import orion_seguranca
        return orion_seguranca.consultar_audit(
            limite=limite,
            tool_filtro=tool_filtro,
            apenas_bloqueados=apenas_bloqueados,
        )
    except Exception as e:
        return {"ok": False, "erro": str(e)}


SCHEMA = [
        {
            "type": "function",
            "function": {
                "name": "consultar_audit_log",
                "description": "Consulta o registro imutável de todas as ferramentas executadas pela Lyra (audit log). Use para responder 'o que você fez ontem?', 'quais comandos foram executados?', ou para auditar ações suspeitas.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limite":           {"type": "integer", "description": "Número de registros a retornar (default 50)."},
                        "tool_filtro":      {"type": "string",  "description": "Filtra por nome de ferramenta específica (ex: 'executar_comando')."},
                        "apenas_bloqueados": {"type": "boolean", "description": "Se true, retorna só as chamadas que foram bloqueadas por rate limit."},
                    },
                },
            },
        },
]


MAP = {
    "consultar_audit_log": consultar_audit_log,
}
