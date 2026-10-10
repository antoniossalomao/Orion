"""`orion ponte` (e `python -m orion.ponte`): sobe a ponte de desktop, ou pareia com o servidor."""

from __future__ import annotations

import socket
import sys

from ..config import Settings
from ..secrets import get_secret, set_secret

SEGREDO = "ORION_PONTE_TOKEN"


def parear(settings: Settings) -> int:
    """Cria o token da ponte (escopo `ponte`, só o hash vai para o `auth.db`) e o guarda no cofre
    do sistema. Parear de novo desfaz o pareamento anterior."""
    from ..auth import AuthService

    auth = AuthService(settings.auth_db_path, user=settings.auth_user)
    try:
        token = auth.create_device_token("ponte", socket.gethostname())
    finally:
        auth.close()
    try:
        set_secret(SEGREDO, token)
    except Exception as e:  # noqa: BLE001 — sem cofre no sistema: o usuário guarda à mão
        print(
            f"cofre indisponível ({type(e).__name__}): defina {SEGREDO} no ambiente da ponte "
            f"com este valor (aparece só agora):\n{token}",
            file=sys.stderr,
        )
        return 1
    print(f"ponte pareada: o token ficou no cofre do sistema como {SEGREDO} (escopo 'ponte').")
    return 0


def executar(settings: Settings) -> int:
    token = get_secret(SEGREDO)
    if not token:
        print("a ponte não está pareada: rode `orion ponte --parear` no computador do servidor",
              file=sys.stderr)  # fmt: skip
        return 1
    url = get_secret("ORION_PONTE_URL") or f"http://127.0.0.1:{settings.port}"
    try:
        from .adaptadores import conectar_real, montar
        from .nucleo import Ponte
    except ImportError as e:
        print(f'faltam dependências da ponte ({e.name}): pip install "orion[ponte]"',
              file=sys.stderr)  # fmt: skip
        return 1
    from .config import TeclasInvalidas

    try:
        ponte = Ponte(url, token, montar(settings.screen_ocr_langs), teclas=settings.hotkeys)
    except TeclasInvalidas as e:
        print(f"ORION_HOTKEYS inválido: {e}", file=sys.stderr)
        return 1
    motivo = ponte.rodar(conectar_real)
    return 0 if motivo == "sair" else 2


def main(argv: list[str] | None = None) -> int:
    settings = Settings()
    return parear(settings) if "--parear" in (argv or sys.argv[1:]) else executar(settings)


if __name__ == "__main__":
    sys.exit(main())
