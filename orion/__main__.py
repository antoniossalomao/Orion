"""`orion` / `python -m orion`: sobe o servidor ou faz backup da memória."""

from __future__ import annotations

import argparse
import sqlite3
import sys
from pathlib import Path

from .config import Settings
from .log import setup_logging


def _set_password(settings: Settings, *, from_stdin: bool) -> int:
    import getpass

    from .auth import AuthService, WeakPassword

    if from_stdin:
        senha = sys.stdin.readline().rstrip("\r\n")
    else:
        senha = getpass.getpass("Nova senha do Orion: ")
        if senha != getpass.getpass("Repita a senha: "):
            print("as senhas não conferem", file=sys.stderr)
            return 1
    auth = AuthService(settings.auth_db_path, user=settings.auth_user)
    try:
        auth.set_password(senha)
    except WeakPassword as e:
        print(e, file=sys.stderr)
        return 1
    finally:
        auth.close()
    print(f"senha de '{settings.auth_user}' definida; as sessões abertas foram encerradas")
    return 0


def _esquecer(settings: Settings, consulta: str, sim: bool) -> int:
    from .memory import MemoryStore

    store = MemoryStore(settings.db_path)
    try:
        if consulta.strip().isdigit():
            fato = store.get_fact(int(consulta))
            achados = [fato] if fato else []
        else:
            achados = store.search_facts(consulta)
        if not achados:
            print("nenhum fato casa com isso")
            return 2
        for f in achados:
            print(f"[{f.id}] {f.text}  (fonte: {f.source})")
        if not sim and input(f"apagar {len(achados)} fato(s)? [s/N] ").strip().lower() != "s":
            print("nada foi apagado")
            return 1
        apagados = sum(store.forget_fact(f.id) for f in achados)
    finally:
        store.close()
    print(
        f"{apagados} fato(s) apagado(s) do banco, do índice de busca e dos vetores.\n"
        "Atenção: backups antigos (<dados>/backups), mensagens de conversas e notas do vault "
        "que repetem o fato NÃO foram tocados."
    )
    return 0


def _mcp_check(config: Path) -> int:
    from .mcp_client import McpConfigError, describe, manager_from_file

    try:
        gerente = manager_from_file(config)
    except McpConfigError as e:
        print(e, file=sys.stderr)
        return 1
    if gerente is None:
        print(f"nenhum servidor habilitado em {config}")
        return 0
    try:
        gerente.start()
        print("\n".join(describe(gerente)))
        return 0 if all(s.startswith("ok") for s in gerente.status.values()) else 1
    finally:
        gerente.stop()


def _wake_test(settings: Settings, *, listar: bool, segundos: int) -> int:
    """Diagnóstico da escuta: mostra o nível do microfone e avisa quando a palavra é ouvida.
    Não usa agente, não fala com a rede e não grava nada."""
    import time

    from .wake import FRAME_S, SoundDeviceSource, criar_detector, rms

    try:
        if listar:
            import sounddevice as sd  # type: ignore[import-not-found]

            print(sd.query_devices())
            return 0
        detector = criar_detector(
            settings.wake_engine, settings.wake_model, settings.wake_words, settings.wake_threshold
        )
        fonte = SoundDeviceSource(settings.wake_device)
    except (ValueError, ImportError, OSError) as e:
        print(f"não consegui preparar o teste: {e}", file=sys.stderr)
        return 1
    print(f'escutando {segundos} s com {settings.wake_engine}; diga "Orion" (Ctrl+C sai)')
    inicio, ouvidas, proximo = time.monotonic(), 0, 0.0
    try:
        for quadro in fonte.frames():
            agora = time.monotonic() - inicio
            if agora >= segundos:
                break
            if agora >= proximo:
                proximo = agora + 0.5
                nivel = min(40, int(rms(quadro) / 100))
                print(f"\r  nível {'#' * nivel:<40}", end="", flush=True)
            if detector.feed(quadro):
                ouvidas += 1
                print(f"\r✔ palavra ouvida ({ouvidas}) aos {agora:.1f} s{' ' * 30}")
                detector.reset()
    except KeyboardInterrupt:
        pass
    finally:
        fonte.close()
    print(f"\nfim: {ouvidas} vez(es). Quadro de {FRAME_S * 1000:.0f} ms, nada foi gravado.")
    return 0 if ouvidas else 2


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="orion")
    sub = parser.add_subparsers(dest="cmd")
    sub.add_parser("serve", help="sobe o servidor (padrão)")
    bk = sub.add_parser("backup", help="backup diário da memória (mantém os últimos N)")
    bk.add_argument(
        "--dir", type=Path, default=None, help="pasta de destino (padrão: <dados>/backups)"
    )
    bk.add_argument("--keep", type=int, default=7)
    au = sub.add_parser(
        "autostart", help="gera o arquivo de início automático do sistema (não ativa sozinho)"
    )
    au.add_argument("--plataforma", choices=("windows", "macos", "linux"), default=None)
    au.add_argument("--install", action="store_true", help="grava o arquivo no lugar do sistema")
    au.add_argument("--dir", type=Path, default=None, help="grava aqui em vez do lugar do sistema")
    au.add_argument("--force", action="store_true", help="substitui um arquivo existente")
    pw = sub.add_parser(
        "set-password", help="define ou troca a senha do login (revoga as sessões abertas)"
    )
    pw.add_argument(
        "--stdin", action="store_true", help="lê a senha da entrada padrão (sem confirmação)"
    )
    mc = sub.add_parser(
        "mcp-check", help="sobe os servidores do mcp.json e mostra as ferramentas e suas classes"
    )
    mc.add_argument("--config", type=Path, default=None, help="padrão: <dados>/mcp.json")
    wt = sub.add_parser(
        "wake-test",
        help="testa o microfone e a palavra de ativação (sem agente, sem rede, sem gravar nada)",
    )
    wt.add_argument("--listar", action="store_true", help="lista os microfones e sai")
    wt.add_argument("--segundos", type=int, default=30, help="quanto tempo escutar")
    sub.add_parser(
        "doctor", help="confere instalação, banco, login, chaves, MCP e backup (sem rede)"
    )
    sub.add_parser("skills", help="lista as skills válidas e as rejeitadas, com o motivo")
    ft = sub.add_parser("fatos", help="lista os fatos da memória (com id, fonte e data)")
    ft.add_argument("--duplicados", action="store_true", help="mostra pares quase iguais")
    es = sub.add_parser(
        "esquecer", help="apaga fatos da memória (texto, índice de busca e vetor); pede confirmação"
    )
    es.add_argument("consulta", help="trecho do fato, ou o id (número) mostrado na lista")
    es.add_argument("--sim", action="store_true", help="não pergunta (apaga todos que casarem)")
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

    if args.cmd == "fatos":
        from .memory import MemoryStore

        store = MemoryStore(settings.db_path)
        try:
            if args.duplicados:
                pares = store.duplicate_facts()
                for a, b, j in pares:
                    print(f"{j:.0%}  [{a.id}] {a.text}\n     [{b.id}] {b.text}")
                print(f"{len(pares)} par(es); apague um com `orion esquecer <id>`")
            else:
                for f in store.facts():
                    print(f"[{f.id}] {f.text}  (fonte: {f.source})")
        finally:
            store.close()
        return 0

    if args.cmd == "skills":
        from .skills import SkillCatalog

        cat = SkillCatalog(settings.effective_skills_dir)
        print(f"pasta: {settings.effective_skills_dir}")
        for sk in cat.skills.values():
            print(f"[ok]  {sk.nome}: {sk.descricao[:80]}" + "".join(f"  ({a})" for a in sk.avisos))
        for r in cat.rejeitadas:
            print(f"[rejeitada] {r.pasta}: {r.motivo}")
        if not cat.skills and not cat.rejeitadas:
            print("nenhuma skill (crie <pasta>/<nome>/SKILL.md)")
        return 0

    if args.cmd == "doctor":
        from .doctor import checar, relatorio

        texto, codigo = relatorio(checar(settings))
        print(texto)
        return codigo

    if args.cmd == "esquecer":
        return _esquecer(settings, args.consulta, args.sim)

    if args.cmd == "autostart":
        from .autostart import detectar, instalar, render
        from .config import PROJECT_ROOT

        a = render(
            args.plataforma or detectar(), projeto=PROJECT_ROOT, logs=settings.data_dir / "logs"
        )
        if not (args.install or args.dir):
            print(f"# {a.arquivo}\n{a.conteudo}")
        else:
            try:
                alvo = instalar(a, destino=args.dir, force=args.force)
            except FileExistsError as e:
                print(e, file=sys.stderr)
                return 1
            if a.plataforma == "macos":
                (settings.data_dir / "logs").mkdir(parents=True, exist_ok=True)
            print(f"gravado: {alvo}")
        print(f"para ligar: {a.ativar}\npara desligar: {a.desativar}")
        return 0

    if args.cmd == "wake-test":
        return _wake_test(settings, listar=args.listar, segundos=args.segundos)

    if args.cmd == "mcp-check":
        return _mcp_check(args.config or settings.effective_mcp_config)

    if args.cmd == "set-password":
        return _set_password(settings, from_stdin=args.stdin)

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
