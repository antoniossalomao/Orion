"""
orion_app.py — Launcher do Orion
pywebview (frameless) + WebSocket hub :8765 + ponte para o cérebro :8000
"""

import asyncio
import ctypes
import json
import os
import sys
import threading

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

import webview

try:
    import websockets
    WS_AVAILABLE = True
except ImportError:
    WS_AVAILABLE = False
    print("[ORION] AVISO: websockets não instalado → pip install websockets")

# ── Estado WS ─────────────────────────────────────────────────────────────────
_ws_clients: set = set()
_ws_loop: asyncio.AbstractEventLoop | None = None

_DIR       = os.path.dirname(os.path.abspath(__file__))
INDEX_HTML = os.path.join(_DIR, "index.html")
ICON_PATH  = os.path.join(_DIR, "orion.ico")
WINDOW_TITLE = "Orion"            # FindWindowW depende deste título exato
APP_ID       = "Orion.Desktop"    # agrupa o botão da barra de tarefas


# ── Broadcast genérico (thread-safe) ──────────────────────────────────────────
def _broadcast(data: dict) -> None:
    """Envia qualquer payload JSON para todos os clientes WS conectados."""
    global _ws_loop
    if not WS_AVAILABLE or not _ws_clients or _ws_loop is None:
        return
    payload = json.dumps(data)

    async def _send():
        dead = set()
        for client in list(_ws_clients):
            try:
                await client.send(payload)
            except Exception:
                dead.add(client)
        _ws_clients.difference_update(dead)

    asyncio.run_coroutine_threadsafe(_send(), _ws_loop)


def broadcast_state(state: str, intensity: float = 0.0) -> None:
    _broadcast({"state": state, "intensity": round(float(intensity), 4)})


# ── WS Server (hub — relay mic_engine <-> frontend) ───────────────────────────
async def _ws_handler(websocket, path=None):
    _ws_clients.add(websocket)
    try:
        async for message in websocket:
            # Relay: repassa mensagens de qualquer cliente para todos os outros
            dead = set()
            for client in list(_ws_clients):
                if client is websocket:
                    continue
                try:
                    await client.send(message)
                except Exception:
                    dead.add(client)
            _ws_clients.difference_update(dead)
    except websockets.exceptions.ConnectionClosed:
        pass
    finally:
        _ws_clients.discard(websocket)


async def _ws_serve():
    global _ws_loop
    _ws_loop = asyncio.get_running_loop()
    # 127.0.0.1: regra do projeto — bind explícito IPv4, nunca localhost/0.0.0.0
    async with websockets.serve(_ws_handler, "127.0.0.1", 8765):
        print("[ORION] Hub ws://127.0.0.1:8765 ativo")
        await asyncio.Future()


def _start_ws():
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    loop.run_until_complete(_ws_serve())


# ── API exposta ao JavaScript via pywebview ────────────────────────────────────
class OrionApi:

    def process_command(self, command: str, modelo: str = "auto") -> bool:
        """Recebe texto do frontend -> streama cerebro :8000 -> envia chunks via WS.
        modelo: 'auto' (cascata) | 'groq' | 'gemini' | 'claude' — seletor manual do painel."""
        threading.Thread(target=self._chat_thread, args=(command, modelo), daemon=True).start()
        return True

    def _chat_thread(self, command: str, modelo: str = "auto") -> None:
        _broadcast({"user_text": command})
        broadcast_state("processing", 0.30)
        try:
            import httpx
            with httpx.Client(timeout=60.0) as client:
                with client.stream(
                    "POST",
                    "http://127.0.0.1:8000/chat",
                    json={"texto": command, "modelo": modelo},
                ) as resp:
                    resp.raise_for_status()
                    broadcast_state("speaking", 0.0)
                    for line in resp.iter_lines():
                        if not line.startswith("data: "):
                            continue
                        raw = line[6:].strip()
                        if raw == "[DONE]":
                            break
                        try:
                            d = json.loads(raw)
                            chunk = d.get("text", "")
                            if chunk:
                                _broadcast({"ai_chunk": chunk})
                            tier = d.get("tier")
                            if tier:
                                _broadcast({"tier": tier})
                        except Exception:
                            pass
        except Exception as e:
            _broadcast({"ai_chunk": "[cérebro offline]"})
            print(f"[ORION] process_command erro: {e}")

        broadcast_state("idle", 0.0)

    def toggle_maximize(self) -> None:
        """Alterna entre fullscreen e janela normal."""
        try:
            webview.windows[0].toggle_fullscreen()
        except Exception as e:
            print(f"[ORION] toggle_maximize: {e}")

    def minimize_app(self) -> None:
        """Minimiza para a barra de tarefas."""
        try:
            webview.windows[0].minimize()
        except AttributeError:
            # Fallback Win32 para pywebview < 4.4
            try:
                hwnd = ctypes.windll.user32.FindWindowW("Chrome_WidgetWin_1", WINDOW_TITLE)
                if not hwnd:
                    hwnd = ctypes.windll.user32.FindWindowW(None, WINDOW_TITLE)
                if hwnd:
                    ctypes.windll.user32.ShowWindow(hwnd, 6)  # SW_MINIMIZE = 6
            except Exception as e2:
                print(f"[ORION] minimize fallback erro: {e2}")
        except Exception as e:
            print(f"[ORION] minimize_app: {e}")

    def close_app(self) -> None:
        print("[ORION] Encerrando...")
        webview.windows[0].destroy()
        sys.exit(0)


# ── Ícone Win32 (WM_SETICON + AppUserModelID) ─────────────────────────────────
def _apply_win32_icon():
    """
    Define o ícone da janela na taskbar via Win32 API.
    Chamado em thread daemon 2s após o start para a janela já existir.
    """
    import time
    time.sleep(2.0)
    try:
        # Agrupa o botão da taskbar com ID único (impede herdar ícone do Python)
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(APP_ID)

        user32 = ctypes.windll.user32
        hwnd   = user32.FindWindowW(None, WINDOW_TITLE)
        if not hwnd:
            print("[ORION] Ícone: janela não encontrada via FindWindow")
            return

        LR_LOADFROMFILE = 0x0010
        IMAGE_ICON      = 1
        hicon = user32.LoadImageW(
            None, ICON_PATH, IMAGE_ICON, 0, 0, LR_LOADFROMFILE
        )
        if not hicon:
            print(f"[ORION] Ícone: LoadImageW falhou ({ICON_PATH})")
            return

        WM_SETICON = 0x0080
        user32.SendMessageW(hwnd, WM_SETICON, 0, hicon)   # ICON_SMALL
        user32.SendMessageW(hwnd, WM_SETICON, 1, hicon)   # ICON_BIG
        print(f"[ORION] Ícone aplicado: {ICON_PATH}")
    except Exception as e:
        print(f"[ORION] Ícone erro: {e}")


# ── Launch ─────────────────────────────────────────────────────────────────────
def launch():
    # AppUserModelID antes de qualquer janela para garantir agrupamento correto
    try:
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(APP_ID)
    except Exception:
        pass

    threading.Thread(target=_start_ws, daemon=True).start()
    threading.Thread(target=_apply_win32_icon, daemon=True).start()

    try:
        user32 = ctypes.windll.user32
        user32.SetProcessDPIAware()
        w = user32.GetSystemMetrics(0)
        h = user32.GetSystemMetrics(1)
    except Exception:
        w, h = 1920, 1080

    # Monitor primário explícito — com mais de uma tela conectada (ex: uma
    # secundária vertical), o fullscreen=True sozinho às vezes abre a janela
    # inteira na tela errada (bug real visto em 04/08/2026: sidebar cortada,
    # conteúdo espremido — a janela tinha ido pro monitor secundário 1080x1920
    # deslocado). webview.screens[0] é sempre o monitor primário.
    tela_primaria = webview.screens[0] if webview.screens else None
    print(f"[ORION] Monitor: {w}x{h} | tela primária: {tela_primaria} | {INDEX_HTML}")

    webview.create_window(
        title=WINDOW_TITLE,
        url=INDEX_HTML,
        js_api=OrionApi(),
        width=w,
        height=h,
        x=0,
        y=0,
        screen=tela_primaria,
        resizable=True,
        frameless=True,
        easy_drag=False,
        background_color="#030409",
        fullscreen=True,
    )
    webview.start(debug=False)


if __name__ == "__main__":
    launch()
