"""Teste de fumaça do Orion empacotado: sobe o executável de verdade e confere o essencial.

    python scripts/smoke_exe.py dist/orion/orion.exe      # Windows
    python scripts/smoke_exe.py dist/orion/orion          # Linux/macOS

Confere: /health, a interface em /ui/, o login `admin` com a senha de fábrica, o aviso de senha de
fábrica (só depois do login), a rota protegida, a troca de senha e que a de fábrica deixa de valer.
Só stdlib (roda no Python do runner, sem instalar nada).
"""

from __future__ import annotations

import http.cookiejar
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def porta_livre() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main(exe: str) -> int:
    porta = porta_livre()
    dados = tempfile.mkdtemp(prefix="orion-smoke-")
    env = {
        **os.environ,
        "ORION_PORT": str(porta),
        "ORION_DATA_DIR": dados,
        "ORION_NO_BROWSER": "1",
        "ORION_JOBS_ENABLED": "false",
        "ORION_MCP_ENABLED": "false",
    }
    log = open(os.path.join(dados, "saida.log"), "wb")
    proc = subprocess.Popen([exe], env=env, stdout=log, stderr=subprocess.STDOUT)  # noqa: S603
    base = f"http://127.0.0.1:{porta}"
    jar = http.cookiejar.CookieJar()
    abrir = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar)).open

    def req(caminho: str, corpo: dict | None = None):
        r = urllib.request.Request(  # noqa: S310
            base + caminho,
            data=json.dumps(corpo).encode() if corpo is not None else None,
            headers={"Content-Type": "application/json"} if corpo is not None else {},
        )
        try:
            with abrir(r, timeout=20) as resp:
                bruto = resp.read()
                tipo = resp.headers.get("content-type", "")
                return resp.status, json.loads(bruto) if "json" in tipo else bruto.decode()
        except urllib.error.HTTPError as e:
            return e.code, None

    falhas: list[str] = []

    def conferir(ok: bool, descricao: str) -> None:
        print(("ok   " if ok else "FALHA"), descricao)
        if not ok:
            falhas.append(descricao)

    try:
        for _ in range(240):  # o primeiro start de um .exe pode levar um tempo (antivírus)
            if proc.poll() is not None:
                break
            try:
                if req("/health")[0] == 200:
                    break
            except OSError:
                time.sleep(0.5)
        status, saude = req("/health")
        conferir(status == 200 and saude and saude["status"] == "ok", "/health responde")
        status, pagina = req("/ui/")
        conferir(
            status == 200 and "Orion" in str(pagina), "interface em /ui/ (index.html empacotado)"
        )
        status, _ = req("/ui/js/app.js")
        conferir(status == 200, "arquivos do front empacotados (js/app.js)")
        status, st = req("/auth/status")
        conferir(
            status == 200
            and st == {"configured": True, "authenticated": False, "token_auth": False},
            "senha de fábrica criada e não anunciada antes do login",
        )
        conferir(req("/approvals")[0] == 401, "rota protegida pede login (401)")
        conferir(
            req("/auth/login", {"usuario": "admin", "senha": "errada"})[0] == 401,
            "senha errada: 401",
        )
        status, _ = req("/auth/login", {"usuario": "admin", "senha": "261210@"})
        conferir(status == 200, "login admin / senha de fábrica")
        conferir(req("/approvals")[0] == 200, "rota protegida abre com a sessão")
        conferir(
            req("/auth/status")[1].get("default_password") is True,
            "avisa que a senha é a de fábrica",
        )
        nova = "uma-senha-nova-bem-longa-1"
        status, _ = req("/auth/password", {"senha_atual": "261210@", "nova": nova})
        conferir(status == 200, "troca a senha")
        conferir(
            req("/auth/status")[1].get("default_password") is False, "aviso some depois da troca"
        )
        jar.clear()
        conferir(
            req("/auth/login", {"usuario": "admin", "senha": "261210@"})[0] == 401,
            "senha de fábrica não vale mais",
        )
        conferir(
            req("/auth/login", {"usuario": "admin", "senha": nova})[0] == 200,
            "entra com a senha nova",
        )
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
        log.close()
    if falhas:
        print("\n--- saída do Orion ---")
        print(
            open(os.path.join(dados, "saida.log"), encoding="utf-8", errors="replace").read()[
                -3000:
            ]
        )
        return 1
    print("\ntudo certo")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
