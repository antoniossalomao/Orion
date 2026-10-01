"""tools/specialist.py — Delegation tools: consult a local/cloud specialist model and drive sub-agent swarms."""

import json

from .notifications import notificar_usuario


def consultar_especialista(problema: str, nivel: str, aprovado: bool = False) -> dict:
    """Delega uma tarefa complexa para um modelo especialista (Local ou Nuvem)."""
    if nivel == "cloud" and not aprovado:
        return {
            "status": "BLOQUEADO",
            "mensagem": "AVISO DE SEGURANÇA: O uso de nuvem (Claude/OpenAI) quebra a Diretiva Nº 2 e requer aprovação explícita. Pergunte ao usuário se ele aprova o envio do problema para a nuvem. Se ele disser sim, chame esta ferramenta novamente com aprovado=True."
        }

    if nivel == "cloud":
        # Exceção consciente à Diretiva Nº 2 (decidida com Antônio em 24/06/2026):
        # delega pra Claude Code (Sonnet) via CLI headless, com agente completo
        # (acesso a arquivo/shell no projeto), avisando o usuário antes de chamar.
        notificar_usuario(
            titulo="Lyra → Claude Code",
            mensagem=f"Delegando para o Claude (Sonnet): {problema[:120]}",
            urgencia="normal",
        )
        try:
            import shutil
            import subprocess
            claude_path = shutil.which("claude")
            if not claude_path:
                return {"erro": "CLI 'claude' não encontrado no PATH.", "ok": False}

            resultado = subprocess.run(
                [claude_path, "-p", problema, "--model", "sonnet",
                 "--allow-dangerously-skip-permissions"],
                cwd=r"C:\Orion",
                capture_output=True,
                text=True,
                timeout=600,
            )
            if resultado.returncode != 0:
                return {"erro": resultado.stderr.strip() or "Claude Code retornou erro.",
                        "status": "FALHA"}
            return {
                "status": "SUCESSO",
                "especialista": "Claude (Sonnet, agente completo via Claude Code)",
                "resposta_do_especialista": resultado.stdout.strip(),
            }
        except subprocess.TimeoutExpired:
            return {"erro": "Claude Code excedeu o tempo limite (10min).", "ok": False}
        except Exception as e:
            return {"erro": str(e), "ok": False}

    # Delegar para Especialista Local (Qwen Coder)
    try:
        from ollama import Client
        c = Client(host='http://127.0.0.1:11434')
        res = c.chat(
            model='qwen2.5-coder:7b',
            messages=[
                {"role": "system", "content": "Você é um Especialista Sênior em Engenharia de Software. Resolva o problema a seguir escrevendo o código mais otimizado, seguro e robusto possível."},
                {"role": "user", "content": problema}
            ]
        )
        return {
            "status": "SUCESSO",
            "especialista": "qwen2.5-coder:7b (Local)",
            "resposta_do_especialista": res['message']['content']
        }
    except Exception as e:
        return {"erro": str(e)}

def _enxame_req(method: str, path: str, payload: dict | None = None) -> dict:
    """Chama o endpoint de enxame no cerebro_maestro (:8000) de forma síncrona."""
    import urllib.request
    import urllib.error
    url = f"http://127.0.0.1:8000{path}"
    try:
        data = json.dumps(payload).encode() if payload else None
        req  = urllib.request.Request(
            url, data=data, method=method,
            headers={"Content-Type": "application/json"} if data else {},
        )
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read().decode())
    except urllib.error.HTTPError as e:
        return {"erro": f"HTTP {e.code}: {e.read().decode()[:300]}"}
    except Exception as e:
        return {"erro": f"Falha ao chamar cerebro: {e}"}


def criar_enxame(objetivo: str, subtarefas: list, max_paralelo: int = 3) -> dict:
    """Cria um enxame de sub-agentes paralelos via HTTP no cerebro_maestro (:8000)."""
    return _enxame_req("POST", "/enxame",
                       {"objetivo": objetivo, "subtarefas": subtarefas, "max_paralelo": max_paralelo})


def status_enxame(enxame_id: str) -> dict:
    """Consulta o progresso de um enxame via HTTP no cerebro_maestro (:8000)."""
    return _enxame_req("GET", f"/enxame/{enxame_id}")


def consolidar_enxame(enxame_id: str) -> dict:
    """Consolida os resultados de um enxame via HTTP no cerebro_maestro (:8000)."""
    return _enxame_req("POST", f"/enxame/{enxame_id}/consolidar")


SCHEMA = [
        {
            "type": "function",
            "function": {
                "name": "criar_enxame",
                "description": (
                    "Cria um enxame de sub-agentes paralelos para executar múltiplas sub-tarefas independentes ao mesmo tempo. "
                    "Use quando o usuário pedir algo que pode ser dividido em partes paralelas (ex: resumir 3 PDFs, pesquisar X em 4 fontes). "
                    "Retorna imediatamente — as sub-tarefas rodam em background. Use status_enxame para acompanhar."
                ),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "objetivo":     {"type": "string", "description": "Descrição do objetivo geral do enxame."},
                        "subtarefas":   {"type": "array", "items": {"type": "string"},
                                         "description": "Lista de sub-tarefas independentes (max 20). Cada item deve ser auto-suficiente."},
                        "max_paralelo": {"type": "integer", "default": 3,
                                         "description": "Máx de sub-tarefas simultâneas (1-6). Padrão 3."},
                    },
                    "required": ["objetivo", "subtarefas"],
                },
            },
        },
        {
            "type": "function",
            "function": {
                "name": "status_enxame",
                "description": "Retorna o progresso de um enxame: quantas sub-tarefas estão pendentes, rodando, concluídas ou com erro.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "enxame_id": {"type": "string", "description": "ID do enxame retornado por criar_enxame."},
                    },
                    "required": ["enxame_id"],
                },
            },
        },
        {
            "type": "function",
            "function": {
                "name": "consolidar_enxame",
                "description": "Consolida os resultados de todas as sub-tarefas de um enxame num único texto coeso via LLM. Só funciona quando o enxame estiver concluído.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "enxame_id": {"type": "string", "description": "ID do enxame a consolidar."},
                    },
                    "required": ["enxame_id"],
                },
            },
        },
        {
            "type": "function",
            "function": {
                "name": "consultar_especialista",
                "description": "Delega um problema complexo (código, tarefa difícil, pesquisa profunda) para um Agente Especialista local ou na nuvem.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "problema": {"type": "string", "description": "Descrição detalhada da tarefa/desafio."},
                        "nivel":    {"type": "string", "enum": ["local", "cloud"], "description": "local = Qwen Coder (sem rede); cloud = Claude (Sonnet) via Claude Code, com agente completo (lê/edita arquivos e roda comandos no projeto) — quebra a Diretiva Nº 2, exige aprovado=True."},
                        "aprovado": {"type": "boolean", "description": "Use True apenas se o usuário tiver explicitamente autorizado o uso da nuvem (ou se o usuário já pediu nessa mesma mensagem para usar o Claude)."}
                    },
                    "required": ["problema", "nivel"]
                }
            }
        },
]


MAP = {
    "consultar_especialista": consultar_especialista,
    "criar_enxame": criar_enxame,
    "status_enxame": status_enxame,
    "consolidar_enxame": consolidar_enxame,
}
