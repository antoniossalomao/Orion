"""`orion` / `python -m orion`: sobe o servidor ou faz backup da memória."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .config import Settings
from .log import setup_logging


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="orion")
    sub = parser.add_subparsers(dest="cmd")
    sub.add_parser("serve", help="sobe o servidor (padrão)")
    bk = sub.add_parser("backup", help="backup diário da memória (mantém os últimos N)")
    bk.add_argument(
        "--dir", type=Path, default=None, help="pasta de destino (padrão: <dados>/backups)"
    )
    bk.add_argument("--keep", type=int, default=7)
    args = parser.parse_args(argv)

    settings = Settings()
    setup_logging(settings.log_level, settings.log_json)

    if args.cmd == "backup":
        from .memory import MemoryStore

        store = MemoryStore(settings.db_path)
        try:
            feito = store.daily_backup(args.dir or settings.data_dir / "backups", manter=args.keep)
        finally:
            store.close()
        print(f"backup: {feito}" if feito else "backup de hoje já existe")
        return 0

    import uvicorn

    from .app import create_app

    uvicorn.run(create_app(settings), host=settings.host, port=settings.port, log_config=None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
