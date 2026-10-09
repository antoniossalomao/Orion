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


def _transcrever(settings: Settings, alvo: str, titulo: str, sim: bool) -> int:
    from . import saidas
    from .app import transcriber_from_settings
    from .capture import Capturer
    from .media_transcribe import MediaError, transcrever_arquivo, transcrever_link
    from .memory import MemoryStore

    tr = transcriber_from_settings(settings)
    if tr is None:
        print("sem ORION_TRANSCRIBE_API_KEY: não há provedor de transcrição", file=sys.stderr)
        return 1
    if settings.vault_dir is None:
        print("defina ORION_VAULT_DIR: a nota vai para o 00 Inbox do vault", file=sys.stderr)
        return 1
    e_link = alvo.lower().startswith(("http://", "https://"))
    origem = "baixado do link" if e_link else f"de '{Path(alvo).name}'"
    pergunta = (
        f"O áudio {origem} será ENVIADO ao provedor de transcrição ({settings.transcribe_url}). "
        "Continuar? [s/N] "
    )
    if not sim and input(pergunta).strip().lower() != "s":
        print("nada foi enviado")
        return 1
    capturador = Capturer(settings.vault_dir, settings.capture_folder)
    progresso = lambda i, n: print(f"transcrevendo parte {i}/{n}…")  # noqa: E731
    store = MemoryStore(settings.db_path)
    saidas.definir_destino(store.add_external_call)  # o áudio sai do computador (regra 47)
    try:
        if e_link:
            print("baixando o áudio…")
            nota = transcrever_link(alvo, tr, capturador, titulo=titulo, progresso=progresso)
        else:
            nota = transcrever_arquivo(
                Path(alvo), tr, capturador, titulo=titulo, progresso=progresso
            )
    except MediaError as e:
        print(e, file=sys.stderr)
        return 1
    finally:
        saidas.definir_destino(None)
        store.close()
    print(f"nota criada: {nota.name}")
    return 0


def _modos(settings: Settings, args: argparse.Namespace) -> int:
    """Pânico e não perturbe pela linha de comando (regra 48). O estado mora no banco: o servidor
    que estiver de pé obedece em segundos, e o audit registra entrada e saída."""
    from datetime import datetime

    from .memory import MemoryStore
    from .memory.ops import Operations
    from .modos import ModoError, Modos

    store = MemoryStore(settings.db_path)
    try:
        modos = Modos(store, dnd_at=settings.dnd_at, audit=Operations(store).audit_add)
        if args.cmd == "panico":
            if args.estado:
                print("modo pânico LIGADO" if modos.panico() else "modo pânico desligado")
            elif args.sair:
                try:
                    saiu = modos.sair_panico("cli")
                except ModoError as e:
                    print(e, file=sys.stderr)
                    return 1
                print("modo pânico desligado" if saiu else "o modo pânico não estava ligado")
            else:
                modos.entrar_panico("cli")
                print(
                    "MODO PÂNICO LIGADO: ferramentas de rede e de execução cortadas, memória da "
                    "tela, escuta e jobs de rede parados. Nada volta sozinho: "
                    "`orion panico --sair`."
                )
            return 0
        if args.sair:
            modos.desligar_nao_perturbe()
            print("não perturbe manual desligado")
        elif args.ate:
            try:
                fim = modos.ligar_nao_perturbe(args.ate)
            except ValueError as e:
                print(e, file=sys.stderr)
                return 1
            print(f"não perturbe até {datetime.fromtimestamp(fim):%d/%m %H:%M}")
        else:
            e = modos.estado()
            print("não perturbe " + ("ligado" if e["nao_perturbe"] else "desligado"))
        return 0
    finally:
        store.close()


def _plugin(settings: Settings, acao: str, alvo: str | None) -> int:
    from .plugins import PluginError, PluginStore, describe

    loja = PluginStore(settings.effective_plugins_dir)
    if acao == "listar":
        for p in loja.lista():
            d = describe(p)
            print(f"{d['nome']} {d['versao']} [{d['estado']}] — {d['descricao']}")
            print(f"   skills: {d['skills']}")
            for sv in d["servidores"]:
                print(
                    f"   MCP {sv['nome']}: {sv['transporte']} → {sv['executa']} "
                    f"(risco padrão {sv['risco_padrao']})"
                )
        if not loja.lista():
            print(f"nenhum plugin em {loja.raiz}")
        return 0
    if not alvo:
        print("faltou a pasta ou o nome do plugin", file=sys.stderr)
        return 1
    try:
        if acao in ("instalar", "atualizar"):
            p = loja.instalar(Path(alvo), atualizar=acao == "atualizar")
            d = describe(p)
            print(f"{p.nome} {p.versao} instalado, SEM concessão. Isto é o que ele liberaria:")
            print(f"   skills: {d['skills']}")
            for sv in d["servidores"]:
                print(f"   MCP {sv['nome']}: {sv['transporte']} → {sv['executa']}")
                livres = sv["ferramentas"] or f"(todas com risco {sv['risco_padrao']})"
                print(f"      ferramentas: {livres}")
            print(
                f"Para liberar: orion plugin conceder {p.nome} (vale depois de reiniciar o Orion)"
            )
        elif acao == "conceder":
            loja.conceder(alvo)
            print(f"{alvo}: concedido; reinicie o Orion para valer")
        elif acao == "revogar":
            print(
                "revogado; reinicie o Orion"
                if loja.revogar(alvo)
                else "esse plugin não tinha concessão"
            )
        else:
            print("removido" if loja.remover(alvo) else "plugin não encontrado")
    except PluginError as e:
        print(e, file=sys.stderr)
        return 1
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
    tr = sub.add_parser(
        "transcrever",
        help="transcreve áudio ou vídeo para uma nota no 00 Inbox (o áudio vai ao provedor)",
    )
    tr.add_argument("arquivo", help="arquivo local, ou um link http(s) (precisa do yt-dlp)")
    tr.add_argument("--titulo", default="", help="título da nota (padrão: nome do arquivo)")
    tr.add_argument("--sim", action="store_true", help="não pergunta antes de enviar o áudio")
    pn = sub.add_parser(
        "panico", help="modo pânico: corta rede, execução, tela e escuta (só sai com --sair)"
    )
    pn.add_argument("--sair", action="store_true", help="desliga o modo pânico")
    pn.add_argument("--estado", action="store_true", help="só mostra se está ligado")
    nd = sub.add_parser(
        "nao-perturbe", help="segura avisos não urgentes até um horário (HH:MM) ou --sair"
    )
    nd.add_argument("ate", nargs="?", help="até quando, HH:MM (ex.: 07:00)")
    nd.add_argument("--sair", action="store_true", help="desliga o não perturbe manual")
    tl = sub.add_parser("tela", help="memória da tela: estado, ou apagar tudo o que foi guardado")
    tl.add_argument("--limpar", action="store_true", help="apaga todo o texto de tela guardado")
    pl = sub.add_parser(
        "plugin", help="plugins locais: listar, instalar, conceder, revogar, remover"
    )
    pl.add_argument(
        "acao", choices=("listar", "instalar", "atualizar", "conceder", "revogar", "remover")
    )
    pl.add_argument("alvo", nargs="?", help="pasta do plugin (instalar/atualizar) ou o nome")
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

    if args.cmd == "plugin":
        return _plugin(settings, args.acao, args.alvo)

    if args.cmd in ("panico", "nao-perturbe"):
        return _modos(settings, args)

    if args.cmd == "tela":
        from .memory import MemoryStore

        store = MemoryStore(settings.db_path)
        try:
            if args.limpar:
                print(f"{store.clear_screen()} registro(s) de tela apagado(s)")
            else:
                estado = "ligada" if settings.screen_memory else "desligada (ORION_SCREEN_MEMORY)"
                pausa = " e pausada" if store.counter_get("tela:pausa") else ""
                print(
                    f"memória da tela {estado}{pausa}: {store.screen_count()} registro(s), "
                    f"guarda {settings.screen_retention_days} dia(s)"
                )
        finally:
            store.close()
        return 0

    if args.cmd == "transcrever":
        return _transcrever(settings, args.arquivo, args.titulo, args.sim)

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
