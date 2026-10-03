"""`orion` / `python -m orion`: sobe o servidor ou faz backup da memória."""

from __future__ import annotations

import argparse
import sqlite3
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
    rs = sub.add_parser("restore", help="restaura um backup no lugar do banco (confere antes)")
    rs.add_argument("arquivo", type=Path, help="backup .db (veja <dados>/backups)")
    rs.add_argument("--force", action="store_true", help="substitui o banco atual, se existir")
    im = sub.add_parser("import-surreal", help="importa o export do SurrealDB do legado")
    im.add_argument(
        "pasta", type=Path, help="pasta com evento.json, sessao.json... (backup_memoria)"
    )
    im.add_argument(
        "--assistente",
        action="append",
        default=["Orion"],
        help="nome de ator que é fala do Orion (repita p/ nomes antigos)",
    )
    vf = sub.add_parser(
        "verify-export",
        help="fase 0: confere o export do legado (contagens, ensaio de importação e restauração)",
    )
    vf.add_argument("pasta", type=Path, help="pasta gerada por backup_memoria")
    vf.add_argument("--assistente", action="append", default=[], help="nome de ator do assistente")
    vf.add_argument(
        "--env", type=Path, default=None, help=".env copiado do PC (só confere os nomes)"
    )
    vf.add_argument("--google-auth", type=Path, default=None, help="pasta google_auth/ copiada")
    vf.add_argument("--vault", type=Path, default=None, help="vault do Obsidian copiado")
    vf.add_argument("--sem-ensaio", action="store_true", help="só conta o export")
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

    if args.cmd == "restore":
        from .memory import MemoryStore

        try:
            contagens = MemoryStore.verify_backup(args.arquivo)
        except (OSError, ValueError, sqlite3.DatabaseError) as e:
            print(f"backup recusado: {e}", file=sys.stderr)
            return 1
        if settings.db_path.exists() and not args.force:
            print(
                f"{settings.db_path} já existe; pare o Orion e use --force para substituir "
                "(o banco atual é perdido: faça `orion backup` antes)",
                file=sys.stderr,
            )
            return 1
        MemoryStore.restore(args.arquivo, settings.db_path).close()
        print(
            f"restaurado em {settings.db_path}: "
            + ", ".join(f"{t}={n}" for t, n in contagens.items() if n)
        )
        return 0

    if args.cmd == "verify-export":
        from .migration import verify_export

        rel = verify_export(
            args.pasta,
            assistentes=["Orion", *args.assistente],
            env_file=args.env,
            google_auth=args.google_auth,
            vault=args.vault,
            ensaio=not args.sem_ensaio,
        )
        print("\n".join(rel.linhas()))
        return 0 if rel.ok else 1

    if args.cmd == "import-surreal":
        from .memory import MemoryStore
        from .memory.importer import import_surreal_export

        store = MemoryStore(settings.db_path)
        try:
            print(import_surreal_export(store, args.pasta, args.assistente).resumo())
        finally:
            store.close()
        return 0

    import uvicorn

    from .app import create_app

    uvicorn.run(create_app(settings), host=settings.host, port=settings.port, log_config=None)
    return 0


if __name__ == "__main__":
    sys.exit(main())
