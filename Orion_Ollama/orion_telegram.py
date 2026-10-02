"""
orion_telegram.py — Bot Telegram bidirecional para o Orion.

Recebe mensagens do Telegram e encaminha para o cerebro_maestro.py (:8000/chat),
coletando os chunks SSE e devolvendo a resposta completa.

SETUP:
  1. Fale com @BotFather no Telegram → /newbot → copie o token
  2. Adicione ao .env:  TELEGRAM_BOT_TOKEN=<seu_token>
  3. Rode: python orion_telegram.py
  4. Procure seu bot no Telegram e envie /start

FUNCIONALIDADES:
  - /start — mensagem de boas-vindas
  - Qualquer texto → resposta do Orion
  - "🎤" indicador de processamento enquanto Orion pensa

MODOS DE SEGURANÇA:
  - TELEGRAM_ALLOWED_USERS: IDs separados por vírgula (OBRIGATÓRIO — vazio = bot recusa iniciar, default-deny desde 03/08/2026)
"""

import sys
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass
import asyncio
import json
import os
import urllib.request
import urllib.error

from dotenv import load_dotenv
load_dotenv(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".env"))

CEREBRO_URL      = "http://127.0.0.1:8000"
BOT_TOKEN     = os.environ.get("TELEGRAM_BOT_TOKEN", "")
ALLOWED_USERS = set(
    int(x.strip()) for x in os.environ.get("TELEGRAM_ALLOWED_USERS", "").split(",")
    if x.strip().isdigit()
)


# ── Comunicação com o Orion ───────────────────────────────────────────────────

def _extrair_texto(linhas) -> str:
    """Junta o texto de um stream SSE do /chat.

    Cada evento é `data: {"text": "..."}` (ou `{"tier": "..."}`, ignorado) e o
    fim é `data: [DONE]`. Versões antigas devolviam o JSON cru ao Telegram.
    """
    pedacos = []
    for bruta in linhas:
        linha = (bruta.decode("utf-8", errors="replace") if isinstance(bruta, bytes) else bruta)
        linha = linha.rstrip("\n\r")
        if not linha.startswith("data: "):
            continue
        dado = linha[6:].strip()
        if dado in ("", "[DONE]"):
            continue
        try:
            evento = json.loads(dado)
        except json.JSONDecodeError:
            continue
        texto = evento.get("text") if isinstance(evento, dict) else None
        if texto:
            pedacos.append(texto)
    return "".join(pedacos).strip()


def _chat_sync(texto: str, modelo: str = "auto") -> str:
    """Chama /chat (SSE streaming) e coleta o texto da resposta."""
    payload = json.dumps({"texto": texto, "modelo": modelo}).encode()
    req = urllib.request.Request(
        f"{CEREBRO_URL}/chat",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:  # noqa: S310 — URL fixa em 127.0.0.1
            return _extrair_texto(resp) or "[Orion não respondeu]"
    except Exception as e:
        return f"[Erro ao contactar Orion: {e}]"


# ── Handlers ─────────────────────────────────────────────────────────────────

async def cmd_start(update, context):
    if update.effective_user.id not in ALLOWED_USERS:
        await update.message.reply_text("Acesso não autorizado.")
        return
    await update.message.reply_text(
        "Orion aqui, seu assistente pessoal.\n"
        "Pode me mandar mensagens de texto aqui que respondo normalmente.\n"
        "Dúvidas? É só perguntar."
    )


async def handle_text(update, context):
    user_id = update.effective_user.id
    if user_id not in ALLOWED_USERS:
        await update.message.reply_text("Acesso não autorizado.")
        return

    texto = update.message.text or ""
    if not texto.strip():
        return

    msg_aguarde = await update.message.reply_text("⏳ Pensando...")

    loop = asyncio.get_running_loop()
    resposta = await loop.run_in_executor(None, _chat_sync, texto)

    await msg_aguarde.delete()

    # Telegram tem limite de 4096 chars por mensagem
    for i in range(0, len(resposta), 4000):
        await update.message.reply_text(resposta[i:i+4000])


# ── Entry point ───────────────────────────────────────────────────────────────

def main():
    if not BOT_TOKEN:
        print("[ERRO] TELEGRAM_BOT_TOKEN não definido no .env")
        print("  1. Fale com @BotFather no Telegram → /newbot → copie o token")
        print("  2. Adicione ao .env:  TELEGRAM_BOT_TOKEN=<token>")
        return

    from telegram.ext import Application, CommandHandler, MessageHandler, filters

    print("[Orion Telegram] Iniciando bot (polling)...")
    print(f"[Orion Telegram] Cérebro: {CEREBRO_URL}")
    if ALLOWED_USERS:
        print(f"[Orion Telegram] Usuários permitidos: {ALLOWED_USERS}")
    else:
        # Default-deny (03/08/2026): antes o allowlist vazio liberava QUALQUER
        # usuário do Telegram — bastava achar o bot pra comandar o Orion com
        # todas as ferramentas (executar_comando incluso). Agora recusa subir.
        print("[ERRO] TELEGRAM_ALLOWED_USERS vazio — o bot NÃO vai iniciar.")
        print("  Sem allowlist, qualquer pessoa que achar o bot comanda o Orion.")
        print("  1. Mande /start pro seu bot e veja seu ID em @userinfobot")
        print("  2. Adicione ao .env:  TELEGRAM_ALLOWED_USERS=<seu_id>")
        return

    app = Application.builder().token(BOT_TOKEN).build()
    app.add_handler(CommandHandler("start", cmd_start))
    app.add_handler(MessageHandler(filters.TEXT & ~filters.COMMAND, handle_text))

    print("[Orion Telegram] Bot rodando. Ctrl+C para parar.")
    app.run_polling(drop_pending_updates=True)


if __name__ == "__main__":
    main()
