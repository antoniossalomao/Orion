"""Linha de base e comparação Rust × Python (B-02 e B-05 do backlog Rust).

Mede, no mesmo banco SQLite (o esquema é compartilhado durante a transição):

1. **Busca** (B-05): latência de FTS5, de vetores e da busca híbrida (p50, p95, p99), a
   memória do índice vetorial (RSS antes e depois da primeira busca vetorial) e a concordância
   dos resultados (os dois lados devem devolver os mesmos itens).
2. **Servidor** (B-02): tempo até o `/health` responder, memória ociosa (RSS) e depois de uma
   rajada de buscas, e a latência HTTP de `/busca`, do `orion-server` em Rust e do Python.
3. **Pacote** (B-02, opcional): tamanho em disco do instalador/pasta dos dois lados.

O embedder é de mentira (palavras → vetor por hash, o mesmo algoritmo nos dois lados): o
benchmark mede o motor de busca e a memória, não a qualidade de um modelo de embeddings (o
spike do modelo BGE-M3 via `ort`/`candle` é outro item do B-05).

Uso (na raiz do repositório):

    # 1. compila o lado Rust (uma vez)
    cargo build --release --manifest-path Orion_Rust/Cargo.toml -p orion-server -p orion-store --example bench_busca

    # 2. mede tudo (Python: `uv sync --python 3.12 --locked` antes, para o servidor subir)
    uv run python scripts/benchmark_rust_python.py --escala media --saida benchmark_resultados

    # só a busca (sem subir servidores) ou só os servidores
    uv run python scripts/benchmark_rust_python.py --so-busca
    uv run python scripts/benchmark_rust_python.py --so-servidor

    # com os tamanhos dos pacotes
    uv run python scripts/benchmark_rust_python.py --pacote-python dist/Orion.exe --pacote-rust "Orion Setup.exe"

Na máquina real (Windows) use um banco real no lugar do sintético: `--db <orion.db>` (a cópia é
medida; o original não é tocado). `--escala` só vale para o banco sintético. O relatório sai em
JSON e Markdown na pasta de `--saida`. Rode com o resto fechado: o RSS é de uma máquina quieta.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import shutil
import socket
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import time
import unicodedata
import urllib.error
import urllib.request
import zlib
from datetime import datetime
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
RUST = RAIZ / "Orion_Rust"
EXE = ".exe" if os.name == "nt" else ""
TOKEN = "benchmark-token-0123456789abcdef"

ESCALAS = {  # fatos, documentos, sessões (de 20 mensagens), consultas
    "pequena": (2_000, 500, 100, 60),
    "media": (20_000, 5_000, 1_000, 150),
    "grande": (100_000, 25_000, 5_000, 300),
}


# ── embedder de mentira (o mesmo algoritmo do bench_busca.rs) ───────────────────────────────
def _plano(texto: str) -> str:
    return unicodedata.normalize("NFKD", texto.lower()).encode("ascii", "ignore").decode()


class FakeEmbedder:
    def __init__(self, dim: int) -> None:
        self.dim = dim

    def embed(self, texts: list[str]) -> list[list[float]]:
        saida = []
        for t in texts:
            v = [0.0] * self.dim
            for tok in re.findall(r"\w+", _plano(t)):
                v[zlib.crc32(tok.encode()) % self.dim] += 1.0
            # desempate: textos de mesmas palavras não podem empatar no cosseno (empate = ordem
            # arbitrária, e os dois lados desempatariam de forma diferente)
            v[zlib.crc32(t.encode() + b"#") % self.dim] += 0.05 * ((zlib.crc32(t.encode()) % 1000) / 1000)
            saida.append(v)
        return saida


# ── banco sintético ─────────────────────────────────────────────────────────────────────────
SILABAS = "ba be bi bo bu ca ce ci co cu da de di do du fa fe fi fo fu ga ge gi go gu la le li lo lu ma me mi mo mu na ne ni no nu pa pe pi po pu ra re ri ro ru sa se si so su ta te ti to tu va ve vi vo vu".split()
ACENTOS = {"a": "á", "e": "é", "o": "ô", "c": "ç"}


def vocabulario(rng: random.Random, n: int = 6000) -> list[str]:
    palavras: set[str] = set()
    while len(palavras) < n:
        p = "".join(rng.choice(SILABAS) for _ in range(rng.randint(2, 4)))
        if rng.random() < 0.08:  # ~8% com acento: o tokenizer remove diacríticos
            for plano, acentuada in ACENTOS.items():
                if plano in p:
                    p = p.replace(plano, acentuada, 1)
                    break
        palavras.add(p)
    return sorted(palavras)


def gerar_banco(destino: Path, fatos: int, docs: int, sessoes: int, dim: int, seed: int) -> list[str]:
    """Cria `destino/orion.db` e devolve as consultas (palavras que existem no banco)."""
    sys.path.insert(0, str(RAIZ))
    from orion.memory import MemoryStore  # noqa: PLC0415 — só o modo de geração precisa

    rng = random.Random(seed)
    vocab = vocabulario(rng)
    pesos = [1.0 / (i + 1) ** 0.9 for i in range(len(vocab))]  # Zipf: poucas palavras comuns

    def frase(n: int) -> str:
        return " ".join(rng.choices(vocab, weights=pesos, k=n))

    destino.mkdir(parents=True, exist_ok=True)
    banco = destino / "orion.db"
    banco.unlink(missing_ok=True)
    consultas: list[str] = []
    s = MemoryStore(banco)
    try:
        for i in range(fatos):
            texto = f"Fato {i}: " + frase(rng.randint(8, 14))
            s.add_fact(texto, f"seed:{i % 50}")
            if i % max(1, fatos // 400) == 0:
                consultas.append(" ".join(texto.split()[2 : 2 + rng.randint(1, 3)]))
        for i in range(docs):
            corpo = "\n\n".join(frase(rng.randint(40, 70)) for _ in range(rng.randint(2, 5)))
            s.index_document(f"notas/{i // 100}/nota-{i}.md", f"Nota {i}", corpo)
        for i in range(sessoes):
            sessao = s.new_session("web", f"Conversa {i}")
            for j in range(20):
                s.add_message(sessao.id, "user" if j % 2 == 0 else "assistant", frase(rng.randint(6, 25)))
    finally:
        s.close()
    # os vetores entram em lote, como o `embed_pending` do Orion faz depois de uma importação
    e = MemoryStore(banco, embedder=FakeEmbedder(dim))
    try:
        while e.embed_pending(512):
            pass
    finally:
        e.close()
    rng.shuffle(consultas)
    return [c for c in consultas if len(c) >= 3]


# ── memória de um processo (RSS), em qualquer sistema ───────────────────────────────────────
def rss_kb(pid: int) -> int:
    try:
        import psutil  # noqa: PLC0415

        p = psutil.Process(pid)
        return int(sum(x.memory_info().rss for x in [p, *p.children(recursive=True)]) / 1024)
    except ImportError:
        pass
    if sys.platform.startswith("linux"):
        for linha in Path(f"/proc/{pid}/status").read_text().splitlines():
            if linha.startswith("VmRSS:"):
                return int(linha.split()[1])
    if sys.platform == "darwin":
        return int(subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0)
    if os.name == "nt":
        import ctypes  # noqa: PLC0415
        from ctypes import wintypes  # noqa: PLC0415

        class PMC(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")
            ]

        h = ctypes.windll.kernel32.OpenProcess(0x1000 | 0x0010, False, pid)  # QUERY_LIMITED | VM_READ
        if not h:
            return 0
        try:
            c = PMC()
            c.cb = ctypes.sizeof(c)
            ctypes.windll.psapi.GetProcessMemoryInfo(h, ctypes.byref(c), c.cb)
            return int(c.WorkingSetSize / 1024)
        finally:
            ctypes.windll.kernel32.CloseHandle(h)
    return 0


def estatisticas(ms: list[float]) -> dict[str, float]:
    ordem = sorted(ms)

    def pct(p: float) -> float:
        return ordem[min(len(ordem) - 1, round((len(ordem) - 1) * p))]

    return {
        "n": len(ordem), "min_ms": ordem[0], "p50_ms": pct(0.5), "p95_ms": pct(0.95),
        "p99_ms": pct(0.99), "max_ms": ordem[-1], "media_ms": statistics.fmean(ordem),
    }  # fmt: skip


# ── lado Python da busca (processo à parte, para o RSS ser só dele) ─────────────────────────
def pausa(fase: str, sem_pausa: bool) -> None:
    if sem_pausa:
        return
    print(f"READY {fase}", flush=True)
    sys.stdin.readline()


def trabalhador_python(args: argparse.Namespace) -> None:
    sys.path.insert(0, str(RAIZ))
    from orion.memory import MemoryStore  # noqa: PLC0415

    consultas = json.loads(Path(args.consultas).read_text(encoding="utf-8"))
    t0 = time.perf_counter()
    store = MemoryStore(args.db, embedder=FakeEmbedder(args.dim))
    abrir_ms = (time.perf_counter() - t0) * 1000
    pausa("aberto", args.sem_pausa)
    so_fts = MemoryStore(args.db)

    def rodar(s: MemoryStore) -> tuple[list[float], list[list[tuple[str, int]]]]:
        ms, topo = [], []
        for r in range(args.reps):
            for q in consultas:
                t = time.perf_counter()
                hits = s.search(q, k=args.k)
                ms.append((time.perf_counter() - t) * 1000)
                if r == 0:
                    topo.append([(h.kind, h.id) for h in hits])
        return ms, topo

    fts_ms, fts_topo = rodar(so_fts)
    t = time.perf_counter()
    store.search(consultas[0], k=args.k)
    frio_ms = (time.perf_counter() - t) * 1000
    pausa("frio", args.sem_pausa)
    hib_ms, hib_topo = rodar(store)
    pausa("quente", args.sem_pausa)
    print("RESULT " + json.dumps({
        "motor": "python", "abrir_ms": abrir_ms, "primeira_busca_vetorial_ms": frio_ms,
        "fts": estatisticas(fts_ms), "hibrida": estatisticas(hib_ms),
        "topo_fts": fts_topo, "topo_hibrida": hib_topo,
    }), flush=True)  # fmt: skip


def medir_trabalhador(cmd: list[str], cwd: Path | None = None) -> dict:
    """Roda um trabalhador do protocolo READY/RESULT e anota o RSS de cada fase."""
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=cwd)
    rss: dict[str, int] = {"inicio": rss_kb(p.pid)}
    resultado = None
    assert p.stdout and p.stdin
    for linha in p.stdout:
        if linha.startswith("READY "):
            rss[linha.split()[1]] = rss_kb(p.pid)
            p.stdin.write("\n")
            p.stdin.flush()
        elif linha.startswith("RESULT "):
            resultado = json.loads(linha[7:])
    p.wait(timeout=60)
    if resultado is None:
        raise RuntimeError(f"{cmd[0]} não devolveu resultado: {p.stderr.read() if p.stderr else ''}"[:600])
    resultado["rss_kb"] = rss
    return resultado


def concordancia(a: list, b: list) -> float:
    """Média do Jaccard dos itens devolvidos por consulta (1.0 = resultados idênticos)."""
    notas = []
    for x, y in zip(a, b, strict=False):
        sx, sy = {tuple(i) for i in x}, {tuple(i) for i in y}
        notas.append(1.0 if not (sx | sy) else len(sx & sy) / len(sx | sy))
    return statistics.fmean(notas) if notas else 0.0


# ── servidores (B-02) ────────────────────────────────────────────────────────────────────────
def porta_livre() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def http(url: str, token: str, timeout: float = 5.0) -> tuple[int, float]:
    t = time.perf_counter()
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "Host": "127.0.0.1"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            r.read()
            return r.status, (time.perf_counter() - t) * 1000
    except urllib.error.HTTPError as e:
        return e.code, (time.perf_counter() - t) * 1000


def _medir(p: subprocess.Popen, t0: float, porta: int, consultas: list[str], n: int) -> dict:
    base = f"http://127.0.0.1:{porta}"
    pronto = None
    while time.perf_counter() - t0 < 90:
        if p.poll() is not None:
            return {"erro": f"saiu com código {p.returncode}: {(p.stderr.read() if p.stderr else '')[-400:]}"}
        try:
            if http(f"{base}/health", TOKEN, 1.0)[0] < 500:
                pronto = time.perf_counter() - t0
                break
        except OSError:
            time.sleep(0.1)
    if pronto is None:
        return {"erro": "o servidor não respondeu ao /health em 90 s"}
    time.sleep(3)  # assenta: a primeira alocação não é a memória ociosa
    ocioso = rss_kb(p.pid)
    ms, falhas = [], 0
    for i in range(n):
        q = urllib.request.quote(consultas[i % len(consultas)])
        status, t = http(f"{base}/busca?q={q}", TOKEN)
        falhas += status != 200
        ms.append(t)
    depois = rss_kb(p.pid)
    return {"inicio_s": pronto, "rss_ocioso_kb": ocioso, "rss_depois_kb": depois, "busca_http": estatisticas(ms), "falhas_http": falhas}


def medir_servidor(nome: str, cmd: list[str], banco: Path, consultas: list[str], n: int, cwd: Path | None) -> dict:
    dados = Path(tempfile.mkdtemp(prefix=f"orion-bench-{nome}-"))
    try:
        shutil.copy(banco, dados / "orion.db")
        porta = porta_livre()
        env = {**os.environ, "ORION_DATA_DIR": str(dados), "ORION_PORT": str(porta), "ORION_HOST": "127.0.0.1",
               "ORION_ADMIN_TOKEN": TOKEN, "ORION_JOBS_ENABLED": "false", "ORION_LOG_JSON": "false",
               "ORION_LOG_LEVEL": "WARNING", "ORION_SERVE_UI": "false",
               # sem canais reais: vazio vence o .env e, sem usuários permitidos, o Telegram nem sobe
               "ORION_TELEGRAM_TOKEN": "", "ORION_TELEGRAM_ALLOWED_USERS": ""}  # fmt: skip
        t0 = time.perf_counter()
        p = subprocess.Popen(cmd, env=env, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        try:
            return _medir(p, t0, porta, consultas, n)
        finally:
            p.terminate()
            try:
                p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
    finally:
        shutil.rmtree(dados, ignore_errors=True)


def tamanho(caminho: str | None) -> float | None:
    if not caminho:
        return None
    p = Path(caminho)
    if p.is_file():
        return p.stat().st_size / 1e6
    if p.is_dir():
        return sum(f.stat().st_size for f in p.rglob("*") if f.is_file()) / 1e6
    return None


# ── relatório ────────────────────────────────────────────────────────────────────────────────
def razao(py: float | None, rs: float | None) -> str:
    return f"{py / rs:.1f}×" if py and rs else "—"


def mb(kb: int | None) -> str:
    return f"{kb / 1024:.0f} MB" if kb else "—"


def relatorio_md(r: dict) -> str:
    L = [f"# Benchmark Rust × Python — {r['quando']}", "",
         f"Máquina: {r['maquina']}. Banco: {r['banco']['descricao']}. Consultas: {r['banco']['consultas']} × {r['banco']['reps']} repetições.", ""]  # fmt: skip
    b = r.get("busca")
    if b:
        L += ["## Busca (B-05)", "", "| modo | Python p50 | Python p95 | Rust p50 | Rust p95 | ganho p50 |", "|---|---|---|---|---|---|"]
        for modo, rotulo in (("fts", "FTS5"), ("hibrida", "FTS5 + vetores")):
            py, rs = b.get("python", {}).get(modo), b.get("rust", {}).get(modo)
            f = lambda d, k: f"{d[k]:.2f} ms" if d else "—"  # noqa: E731
            L.append(f"| {rotulo} | {f(py, 'p50_ms')} | {f(py, 'p95_ms')} | {f(rs, 'p50_ms')} | {f(rs, 'p95_ms')} | {razao(py and py['p50_ms'], rs and rs['p50_ms'])} |")  # fmt: skip
        L += ["", "| memória (RSS) | Python | Rust |", "|---|---|---|"]
        for fase, rotulo in (("aberto", "banco aberto"), ("frio", "depois da 1ª busca vetorial (matriz carregada)"), ("quente", "depois das buscas")):
            L.append(f"| {rotulo} | {mb(b.get('python', {}).get('rss_kb', {}).get(fase))} | {mb(b.get('rust', {}).get('rss_kb', {}).get(fase))} |")
        L += ["", f"Primeira busca vetorial (carga da matriz): Python {b.get('python', {}).get('primeira_busca_vetorial_ms', 0):.0f} ms, Rust {b.get('rust', {}).get('primeira_busca_vetorial_ms', 0):.0f} ms.",
              f"Concordância dos resultados (Jaccard médio por consulta): FTS {b.get('concordancia_fts', 0):.3f}, híbrida {b.get('concordancia_hibrida', 0):.3f}.", ""]  # fmt: skip
    s = r.get("servidor")
    if s:
        L += ["## Servidor (B-02)", "", "| medida | Python | Rust | ganho |", "|---|---|---|---|"]
        py, rs = s.get("python", {}), s.get("rust", {})
        for chave, rotulo, fmt in (("inicio_s", "tempo até o /health", lambda v: f"{v:.2f} s"), ("rss_ocioso_kb", "memória ociosa", mb), ("rss_depois_kb", "memória depois das buscas", mb)):
            a, c = py.get(chave), rs.get(chave)
            L.append(f"| {rotulo} | {fmt(a) if a else py.get('erro', '—')[:60]} | {fmt(c) if c else rs.get('erro', '—')[:60]} | {razao(a, c)} |")
        a, c = py.get("busca_http", {}).get("p50_ms"), rs.get("busca_http", {}).get("p50_ms")
        L.append(f"| /busca HTTP p50 | {f'{a:.1f} ms' if a else '—'} | {f'{c:.1f} ms' if c else '—'} | {razao(a, c)} |")
        a, c = py.get("busca_http", {}).get("p95_ms"), rs.get("busca_http", {}).get("p95_ms")
        L.append(f"| /busca HTTP p95 | {f'{a:.1f} ms' if a else '—'} | {f'{c:.1f} ms' if c else '—'} | {razao(a, c)} |")
        L.append("")
    if r.get("pacote"):
        L += ["## Pacote", "", "| | tamanho |", "|---|---|"] + [f"| {k} | {v:.1f} MB |" for k, v in r["pacote"].items() if v] + [""]
    return "\n".join(L)


# ── principal ────────────────────────────────────────────────────────────────────────────────
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--worker", choices=["python-store"], help=argparse.SUPPRESS)
    ap.add_argument("--db", help="orion.db real (a cópia é medida); sem ele gera um sintético")
    ap.add_argument("--escala", choices=list(ESCALAS), default="media")
    ap.add_argument("--dim", type=int, default=384)
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--k", type=int, default=8)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--consultas", help="JSON com a lista de consultas (sem ele, sorteia do banco sintético)")
    ap.add_argument("--so-busca", action="store_true")
    ap.add_argument("--so-servidor", action="store_true")
    ap.add_argument("--pular-python", action="store_true")
    ap.add_argument("--pular-rust", action="store_true")
    ap.add_argument("--sem-pausa", action="store_true", help=argparse.SUPPRESS)
    ap.add_argument("--requisicoes", type=int, default=200, help="buscas HTTP por servidor")
    ap.add_argument("--rust-bin", default=str(RUST / "target" / "release" / f"orion{EXE}"))
    ap.add_argument("--rust-exemplo", default=str(RUST / "target" / "release" / "examples" / f"bench_busca{EXE}"))
    ap.add_argument("--python-cmd", default=None, help="comando do servidor Python (padrão: este Python -m orion serve)")
    ap.add_argument("--pacote-python")
    ap.add_argument("--pacote-rust")
    ap.add_argument("--saida", default="benchmark_resultados")
    args = ap.parse_args()

    if args.worker == "python-store":
        trabalhador_python(args)
        return 0

    saida = Path(args.saida)
    saida.mkdir(parents=True, exist_ok=True)
    trabalho = Path(tempfile.mkdtemp(prefix="orion-bench-dados-"))
    try:
        if args.db:
            banco = trabalho / "orion.db"
            # cópia consistente (o banco real usa WAL: copiar só o .db perderia páginas do -wal)
            origem = sqlite3.connect(f"file:{args.db}?mode=ro", uri=True)
            destino_db = sqlite3.connect(banco)
            origem.backup(destino_db)
            destino_db.close()
            origem.close()
            dim_real = sqlite3.connect(banco).execute("SELECT value FROM meta WHERE key='embed_dim'").fetchone()
            if dim_real:  # o embedder precisa ter a dimensão do índice do banco
                args.dim = int(dim_real[0])
            descricao = f"cópia de {args.db}"
            if args.consultas:
                consultas = json.loads(Path(args.consultas).read_text(encoding="utf-8"))
            else:
                sys.exit("com --db informe --consultas (JSON com a lista de consultas reais)")
        else:
            fatos, docs, sessoes, n_consultas = ESCALAS[args.escala]
            print(f"gerando o banco sintético ({args.escala}: {fatos} fatos, {docs} notas, {sessoes * 20} mensagens)…", flush=True)
            t = time.perf_counter()
            consultas = gerar_banco(trabalho, fatos, docs, sessoes, args.dim, args.seed)[:n_consultas]
            banco = trabalho / "orion.db"
            descricao = f"sintético ({args.escala}): {fatos} fatos, {docs} notas, {sessoes * 20} mensagens, vetores de {args.dim} dim, {banco.stat().st_size / 1e6:.0f} MB"
            print(f"  pronto em {time.perf_counter() - t:.0f} s", flush=True)
        qarq = trabalho / "consultas.json"
        qarq.write_text(json.dumps(consultas, ensure_ascii=False), encoding="utf-8")

        r: dict = {
            "quando": datetime.now().strftime("%d/%m/%Y %H:%M"),
            "maquina": f"{sys.platform}, {os.cpu_count()} CPUs, Python {sys.version.split()[0]}",
            "banco": {"descricao": descricao, "consultas": len(consultas), "reps": args.reps},
        }
        comum = ["--db", str(banco), "--dim", str(args.dim), "--consultas", str(qarq), "--reps", str(args.reps), "--k", str(args.k)]

        if not args.so_servidor:
            print("medindo a busca…", flush=True)
            busca: dict = {}
            if not args.pular_python:
                busca["python"] = medir_trabalhador([sys.executable, str(Path(__file__).resolve()), "--worker", "python-store", *comum], cwd=RAIZ)
            if not args.pular_rust:
                if not Path(args.rust_exemplo).exists():
                    sys.exit(f"falta {args.rust_exemplo}: rode `cargo build --release -p orion-store --example bench_busca` em Orion_Rust")
                busca["rust"] = medir_trabalhador([args.rust_exemplo, *comum])
            if "python" in busca and "rust" in busca:
                busca["concordancia_fts"] = concordancia(busca["python"]["topo_fts"], busca["rust"]["topo_fts"])
                busca["concordancia_hibrida"] = concordancia(busca["python"]["topo_hibrida"], busca["rust"]["topo_hibrida"])
            for lado in ("python", "rust"):
                for chave in ("topo_fts", "topo_hibrida"):
                    busca.get(lado, {}).pop(chave, None)
            r["busca"] = busca

        if not args.so_busca:
            print("medindo os servidores…", flush=True)
            serv: dict = {}
            if not args.pular_python:
                cmd = args.python_cmd.split() if args.python_cmd else [sys.executable, "-m", "orion", "serve"]
                serv["python"] = medir_servidor("python", cmd, banco, consultas, args.requisicoes, RAIZ)
            if not args.pular_rust:
                if not Path(args.rust_bin).exists():
                    sys.exit(f"falta {args.rust_bin}: rode `cargo build --release -p orion-server` em Orion_Rust")
                serv["rust"] = medir_servidor("rust", [args.rust_bin, "serve"], banco, consultas, args.requisicoes, RAIZ)
            r["servidor"] = serv

        pacote = {"Python": tamanho(args.pacote_python), "Rust": tamanho(args.pacote_rust)}
        if any(pacote.values()):
            r["pacote"] = pacote

        nome = datetime.now().strftime("benchmark-%Y%m%d-%H%M%S")
        (saida / f"{nome}.json").write_text(json.dumps(r, indent=2, ensure_ascii=False), encoding="utf-8")
        md = relatorio_md(r)
        (saida / f"{nome}.md").write_text(md, encoding="utf-8")
        print("\n" + md)
        print(f"\nrelatório em {saida / nome}.json e .md")
        return 0
    finally:
        shutil.rmtree(trabalho, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
