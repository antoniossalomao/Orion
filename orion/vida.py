"""E10 — Vida pessoal: finanças locais, contas, monitor de preço, cápsula, diário, revisão,
escudo de foco, cenários e estilo pela voz.

Regra 61: dado financeiro nunca vai à nuvem. Gasto, conta e preço ficam no banco local; o parser
é por regra (valor e palavra-chave), sem modelo. Regra 62: o escudo de foco só **pergunta**, nunca
bloqueia nem fecha nada. Regra 63: o estado pela voz só muda o estilo da resposta; as médias
moram em memória e não entram na memória do Orion.
"""

from __future__ import annotations

import logging
import random
import re
import shutil
import subprocess
import unicodedata
from collections.abc import Callable, Iterable
from datetime import date, datetime, timedelta
from pathlib import Path
from typing import Any

log = logging.getLogger("orion.vida")

MAX_CENTS = 1_000_000_000  # R$ 10 milhões: acima disso é erro de digitação
BASE_BOLETO = date(1997, 10, 7)  # fator de vencimento do boleto bancário conta a partir daqui
CATEGORIAS: dict[str, tuple[str, ...]] = {
    "mercado": ("mercado", "supermercado", "padaria", "feira", "acougue", "hortifruti"),
    "transporte": (
        "uber",
        "99",
        "gasolina",
        "combustivel",
        "onibus",
        "metro",
        "estacionamento",
        "pedagio",
        "passagem",
    ),
    "alimentação": ("restaurante", "lanche", "ifood", "almoco", "jantar", "cafe", "bar", "pizza"),
    "moradia": ("aluguel", "condominio", "luz", "agua", "internet", "gas", "iptu"),
    "saúde": ("farmacia", "remedio", "medico", "consulta", "plano", "dentista", "exame"),
    "lazer": ("cinema", "jogo", "show", "netflix", "spotify", "viagem", "ingresso"),
}
_GASTO = re.compile(
    r"^\s*(?:gastei|paguei|comprei)\s+(?:r\$\s*)?(\d[\d.,]*)\s*(?:reais)?\s*"
    r"(?:(?:no|na|em|de|com|pro|pra|para|do|da)\s+)?(.*)$",
    re.I,
)
_LINHA = re.compile(r"\d{5}[.\s]?\d{5}\s*\d{5}[.\s]?\d{6}\s*\d{5}[.\s]?\d{6}\s*\d\s*\d{14}")
_PRECO = re.compile(r"r\$\s*(\d{1,3}(?:\.\d{3})*(?:,\d{2})|\d+(?:[.,]\d{2})?)", re.I)


class VidaError(ValueError):
    pass


def _sem_acento(texto: str) -> str:
    return "".join(
        c for c in unicodedata.normalize("NFKD", texto.lower()) if not unicodedata.combining(c)
    )


def parse_valor(bruto: str) -> int:
    """'30' → 3000; '30,5' → 3050; '1.234,56' → 123456; '12.90' → 1290. Recusa zero e negativo."""
    s = bruto.strip().lower().replace("r$", "").replace(" ", "")
    if "," in s:
        s = s.replace(".", "").replace(",", ".")
    elif s.count(".") == 1 and len(s.split(".")[1]) == 2:
        pass  # ponto decimal de duas casas: 12.90
    else:
        s = s.replace(".", "")  # ponto de milhar: 1.500
    try:
        valor = round(float(s) * 100)
    except ValueError:
        raise VidaError(f"valor inválido: {bruto}") from None
    if not 0 < valor <= MAX_CENTS:
        raise VidaError("valor precisa ser positivo e razoável")
    return int(valor)


def categoria_de(texto: str) -> str:
    t = _sem_acento(texto)
    for cat, palavras in CATEGORIAS.items():
        if any(re.search(rf"\b{re.escape(_sem_acento(p))}\b", t) for p in palavras):
            return cat
    return "outros"


def parse_gasto(texto: str) -> dict[str, Any] | None:
    """'gastei 30 no mercado' → {cents, category, description}. Sem valor claro, devolve None."""
    m = _GASTO.match(texto.strip())
    if not m:
        return None
    try:
        cents = parse_valor(m.group(1))
    except VidaError:
        return None
    desc = m.group(2).strip()[:200] or "sem descrição"
    return {"cents": cents, "category": categoria_de(desc), "description": desc}


def parse_linha_digitavel(texto: str) -> dict[str, Any] | None:
    """Boleto bancário (47 dígitos): devolve o código, o valor e o vencimento pelo fator.
    Concessionária (48 dígitos) e formatos sem a linha ficam de fora (None)."""
    m = _LINHA.search(texto)
    if not m:
        return None
    digitos = re.sub(r"\D", "", m.group(0))
    if len(digitos) != 47:
        return None
    fator = int(digitos[33:37])
    valor = int(digitos[37:47])
    due = BASE_BOLETO + timedelta(days=fator) if fator > 0 else None
    return {"barcode": digitos, "cents": valor, "due": due}


def extrair_preco(texto: str) -> int | None:
    """Primeiro 'R$ 1.234,56' de um texto de busca, em centavos."""
    m = _PRECO.search(texto)
    if not m:
        return None
    try:
        return parse_valor(m.group(1))
    except VidaError:
        return None


def avaliar_preco(last: int | None, novo: int, alvo: int | None) -> str | None:
    """'alvo' quando cruza o preço-alvo para baixo; 'queda' quando cai 5% ou mais desde a última."""
    if alvo is not None and novo <= alvo and (last is None or last > alvo):
        return "alvo"
    if last is not None and novo <= last * 0.95:
        return "queda"
    return None


def reais(cents: int) -> str:
    """Formato brasileiro: 'R$ 1.234,56'."""
    return f"R$ {cents / 100:,.2f}".replace(",", "_").replace(".", ",").replace("_", ".")


def _agora() -> float:
    return datetime.now().timestamp()


def _dia(ts: float) -> date:
    return datetime.fromtimestamp(ts).date()


def _ts_fim_do_dia(d: date) -> float:
    return datetime(d.year, d.month, d.day, 23, 59, 59).timestamp()


class Financas:
    """Gastos, resumo mensal e contas. Regra 61: tudo local; nada sai daqui."""

    def __init__(self, memory: Any, *, clock: Callable[[], float] = _agora):
        self.memory, self._clock = memory, clock

    def registrar_gasto(
        self, cents: int, category: str, description: str, *, source: str = "chat"
    ) -> dict[str, Any]:
        if not 0 < cents <= MAX_CENTS:
            raise VidaError("valor precisa ser positivo e razoável")
        with self.memory.transaction() as c:
            cur = c.execute(
                "INSERT INTO transactions(ts, cents, category, description, source)"
                " VALUES (?,?,?,?,?)",
                (self._clock(), cents, category, description[:200], source),
            )
            rid = cur.lastrowid
        return {"id": rid, "cents": cents, "category": category, "description": description}

    def historico_mensal(self, meses: int = 12) -> list[int]:
        """Total de gastos por mês nos últimos `meses` meses com gasto (base da simulação)."""
        rows = self.memory.query(
            "SELECT strftime('%Y-%m', ts, 'unixepoch', 'localtime') AS mes, SUM(cents) AS total"
            " FROM transactions GROUP BY mes ORDER BY mes DESC LIMIT ?",
            (max(1, meses),),
        )
        return [int(r["total"]) for r in rows]

    def apagar_gasto(self, rid: int) -> None:
        """Desfazer de captura (até 10 minutos, como os outros itens da captura)."""
        with self.memory.transaction() as c:
            c.execute(
                "DELETE FROM transactions WHERE id=? AND source IN ('chat','captura')", (rid,)
            )

    def gastos_do_dia(self, dia: date) -> tuple[int, dict[str, int]]:
        rows = self.memory.query(
            "SELECT category, SUM(cents) AS total FROM transactions WHERE ts >= ? AND ts <= ?"
            " GROUP BY category",
            (datetime(dia.year, dia.month, dia.day).timestamp(), _ts_fim_do_dia(dia)),
        )
        por = {r["category"]: int(r["total"]) for r in rows}
        return sum(por.values()), por

    def resumo_mensal(self, ano: int, mes: int) -> dict[str, Any]:
        inicio = datetime(ano, mes, 1).timestamp()
        fim_dia = date(ano + (mes == 12), mes % 12 + 1, 1) - timedelta(days=1)
        rows = self.memory.query(
            "SELECT category, SUM(cents) AS total, COUNT(*) AS n FROM transactions"
            " WHERE ts >= ? AND ts <= ? GROUP BY category ORDER BY total DESC",
            (inicio, _ts_fim_do_dia(fim_dia)),
        )
        por_categoria = {r["category"]: int(r["total"]) for r in rows}
        return {
            "mes": f"{ano:04d}-{mes:02d}",
            "total_cents": sum(por_categoria.values()),
            "por_categoria": por_categoria,
        }

    def cadastrar_conta(
        self, descricao: str, cents: int, due: date, *, barcode: str = "", file: str = ""
    ) -> dict[str, Any]:
        if not 0 <= cents <= MAX_CENTS:
            raise VidaError("valor fora do limite")
        with self.memory.transaction() as c:
            cur = c.execute(
                "INSERT INTO bills(description, cents, due_at, barcode, file) VALUES (?,?,?,?,?)",
                (descricao[:200] or "conta", cents, _ts_fim_do_dia(due), barcode, file[:300]),
            )
            rid = cur.lastrowid
        return {"id": rid, "description": descricao, "cents": cents, "due": due.isoformat()}

    def pagar_conta(self, rid: int) -> dict[str, Any]:
        rows = self.memory.query("SELECT * FROM bills WHERE id=?", (rid,))
        if not rows:
            raise VidaError("conta não encontrada")
        conta = rows[0]
        if conta["paid"]:
            raise VidaError("conta já paga")
        with self.memory.transaction() as c:
            c.execute("UPDATE bills SET paid=1 WHERE id=?", (rid,))
        self.registrar_gasto(
            int(conta["cents"]), "contas", str(conta["description"]), source="conta"
        )
        return {"id": rid, "paga": True}

    def contas_abertas(self) -> list[dict[str, Any]]:
        rows = self.memory.query(
            "SELECT id, description, cents, due_at FROM bills WHERE paid=0 ORDER BY due_at"
        )
        return [
            {
                "id": r["id"],
                "description": r["description"],
                "cents": int(r["cents"]),
                "due": _dia(r["due_at"]).isoformat(),
            }
            for r in rows
        ]

    def avisos_de_contas(
        self,
        agora: float,
        notificar: Callable[[str, str, str], Any],
        dias: Iterable[int] = (3, 1),
    ) -> int:
        """Avisa 3 e 1 dia antes do vencimento, uma vez por dia e por aviso. Devolve quantos."""
        hoje = _dia(agora)
        n = 0
        for r in self.memory.query("SELECT * FROM bills WHERE paid=0"):
            restam = (_dia(r["due_at"]) - hoje).days
            if restam not in set(dias):
                continue
            marca = f"{hoje:%Y%m%d}:{restam}"
            if r["avisado"] == marca:
                continue
            notificar(
                "conta",
                f"{r['description']} vence em {restam} dia(s): {reais(r['cents'])}",
                f"conta:{r['id']}:{marca}",
            )
            with self.memory.transaction() as c:
                c.execute("UPDATE bills SET avisado=? WHERE id=?", (marca, r["id"]))
            n += 1
        return n


class Precos:
    """Monitor de preço (E10.5). `pesquisar(consulta) -> texto` é injetado: a busca cobra cota
    (regra 46) e é quem decide se pode rodar; aqui só se compara o preço."""

    def __init__(self, memory: Any, *, clock: Callable[[], float] = _agora):
        self.memory, self._clock = memory, clock

    def adicionar(self, item: str, consulta: str, alvo_cents: int | None = None) -> dict[str, Any]:
        with self.memory.transaction() as c:
            cur = c.execute(
                "INSERT INTO price_watch(item, query, target_cents) VALUES (?,?,?)",
                (item[:120], consulta[:200], alvo_cents),
            )
            rid = cur.lastrowid
        return {"id": rid, "item": item, "alvo_cents": alvo_cents}

    def listar(self) -> list[dict[str, Any]]:
        rows = self.memory.query("SELECT * FROM price_watch ORDER BY id")
        return [
            {
                "id": r["id"],
                "item": r["item"],
                "alvo_cents": r["target_cents"],
                "ultimo_cents": r["last_cents"],
                "ativo": bool(r["active"]),
            }
            for r in rows
        ]

    def verificar(
        self, pesquisar: Callable[[str], str | None], *, agora: float | None = None
    ) -> list[dict[str, Any]]:
        """Uma consulta por item por dia. Devolve os alertas (queda ou alvo atingido)."""
        hoje = int(_dia(agora if agora is not None else self._clock()).strftime("%Y%m%d"))
        alertas: list[dict[str, Any]] = []
        for r in self.memory.query(
            "SELECT * FROM price_watch WHERE active=1 AND checked_on<?", (hoje,)
        ):
            texto = pesquisar(r["query"])
            novo = extrair_preco(texto) if texto else None
            with self.memory.transaction() as c:
                c.execute("UPDATE price_watch SET checked_on=? WHERE id=?", (hoje, r["id"]))
            if novo is None:
                continue
            last = r["last_cents"]
            motivo = avaliar_preco(last, novo, r["target_cents"])
            historico = [h for h in str(r["history"]).split(";") if h][-29:]
            historico.append(f"{hoje}:{novo}")
            with self.memory.transaction() as c:
                c.execute(
                    "UPDATE price_watch SET last_cents=?, history=? WHERE id=?",
                    (novo, ";".join(historico), r["id"]),
                )
            if motivo:
                alertas.append({"id": r["id"], "item": r["item"], "motivo": motivo, "cents": novo})
        return alertas


def capsula(memory: Any, texto: str, dias: int, contexto: str = "") -> dict[str, Any]:
    """E10.6: 'daqui 6 meses me cobra isso'. Vira lembrete com `kind='capsula'`."""
    if not 1 <= dias <= 3650:
        raise VidaError("a cápsula vai de 1 a 3650 dias")
    due = datetime.now().timestamp() + dias * 86400
    with memory.transaction() as c:
        cur = c.execute(
            "INSERT INTO reminders(title, due_at, note, created_at, kind)"
            " VALUES (?,?,?,?,'capsula')",
            (texto[:200], due, contexto[:2000], datetime.now().timestamp()),
        )
        rid = cur.lastrowid
    return {"id": rid, "titulo": texto[:200], "quando": datetime.fromtimestamp(due).isoformat()}


def simular(
    saldo_cents: int,
    gastos_mensais: list[int],
    extra_mensal_cents: int,
    meses: int,
    *,
    rodadas: int = 10_000,
    seed: int | None = None,
) -> dict[str, int]:
    """E10.7: Monte Carlo com os seus gastos mensais reais como base (reamostragem com reposição).
    Devolve faixas (p10, p50, p90) do saldo final em centavos; nunca um número só."""
    if not gastos_mensais:
        raise VidaError("sem histórico de gastos para simular")
    if not 1 <= meses <= 120:
        raise VidaError("a simulação vai de 1 a 120 meses")
    rng = random.Random(seed)  # noqa: S311 - simulação, não é segurança
    finais = []
    for _ in range(rodadas):
        gasto = sum(rng.choice(gastos_mensais) for _ in range(meses))
        finais.append(saldo_cents - gasto - extra_mensal_cents * meses)
    finais.sort()

    def pct(p: float) -> int:
        return finais[min(len(finais) - 1, int(p * len(finais)))]

    return {"p10": pct(0.10), "p50": pct(0.50), "p90": pct(0.90)}


def montar_diario(
    dia: date,
    *,
    commits: list[str],
    gastos_cents: int,
    gastos_por_categoria: dict[str, int] | None = None,
    resumo: str | None = None,
) -> str:
    """E10.2: nota do dia para o vault. Só entra o que existe; sem nada, a seção vem vazia."""
    linhas_commits = [f"- {c[:200]}" for c in commits[:50]] or ["- nenhum commit"]
    cats = gastos_por_categoria or {}
    linhas_gastos = [f"- {k}: {reais(v)}" for k, v in sorted(cats.items())]
    total = f"{reais(gastos_cents)}"
    descricao = resumo or f"{len(commits)} commit(s), gastos {total}"
    return "\n".join(
        [
            "---",
            "type: diario",
            f"date: {dia.isoformat()}",
            "fonte: orion",
            "tags: [diario]",
            f'resumo: "{descricao[:200].replace(chr(34), chr(39))}"',
            "---",
            "",
            f"# Diário — {dia.isoformat()}",
            "",
            "## Commits",
            *linhas_commits,
            "",
            "## Gastos",
            f"Total do dia: {total}",
            *linhas_gastos,
            "",
            "## Resumo",
            resumo or "Resumo não gerado (ligue o modelo local para um resumo do dia).",
            "",
        ]
    )


def commits_do_dia(repos: Iterable[str | Path], dia: date) -> list[str]:
    """Commits do dia em cada repositório de ORION_REPOS (só leitura; falhar devolve vazio)."""
    git = shutil.which("git")
    if git is None:
        return []
    saida: list[str] = []
    for repo in repos:
        try:
            r = subprocess.run(  # noqa: S603 - argumentos fixos; o caminho vem de ORION_REPOS
                [
                    git,
                    "-C",
                    str(repo),
                    "log",
                    f"--since={dia.isoformat()} 00:00",
                    f"--until={dia.isoformat()} 23:59:59",
                    "--format=%s",
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=10,
                check=False,
            )
        except (OSError, subprocess.SubprocessError):
            continue
        if r.returncode == 0:
            saida += [f"{Path(repo).name}: {linha}" for linha in r.stdout.splitlines() if linha]
    return saida


def escrever_no_vault(vault: Path, pasta: str, nome: str, texto: str) -> Path:
    """Escreve `texto` em `<vault>/<pasta>/<nome>.md`. A pasta é fixa e o nome vem de data."""
    if not re.fullmatch(r"[\w .\-À-ÿ]{1,80}", pasta) or ".." in pasta:
        raise VidaError("pasta inválida")
    if not re.fullmatch(r"[\w\-]{1,40}", nome):
        raise VidaError("nome de arquivo inválido")
    destino_dir = (vault / pasta).resolve()
    if vault.resolve() not in destino_dir.parents:
        raise VidaError("a nota precisa ficar dentro do vault")
    destino_dir.mkdir(parents=True, exist_ok=True)
    arquivo = destino_dir / f"{nome}.md"
    arquivo.write_text(texto, encoding="utf-8")
    return arquivo


def montar_revisao(tarefas: list[str], projetos: list[dict[str, str]]) -> str:
    """E10.1: esqueleto da revisão de domingo com os 5 passos do Sistema de Priorização."""
    abertas = [f"- [ ] {t[:200]}" for t in tarefas[:30]] or ["- nenhuma tarefa aberta"]
    proj = [f"- **{p['nome']}**: próxima ação: {p.get('proxima', '—')}" for p in projetos[:20]]
    return "\n".join(
        [
            "# Revisão semanal",
            "",
            "## 1. Tarefas abertas",
            *abertas,
            "",
            "## 2. Escolher 1 marco",
            "- [ ] marco da semana:",
            "",
            "## 3. Blocos na agenda",
            "- [ ] bloco 1 (proposta, a agenda pede aprovação)",
            "- [ ] bloco 2",
            "",
            "## 4. Ideias para incubação",
            "",
            "## 5. Bloqueios e próxima ação por projeto",
            *(proj or ["- nenhum projeto ativo"]),
            "",
        ]
    )


def e_distracao(alvo: str, lista: Iterable[str]) -> bool:
    """Casa o app ou site em foco com a lista de distrações (sem acento, sem caixa)."""
    a = _sem_acento(alvo)
    return any(item and _sem_acento(item) in a for item in (x.strip() for x in lista))


class Foco:
    """E10.10: bloco de foco em memória. Regra 62: `pergunta_permitida` diz se pode perguntar
    ("isso é do marco da semana?"); no máximo uma pergunta por bloco. Nunca bloqueia nada."""

    def __init__(
        self,
        distracoes: Iterable[str] = (),
        *,
        clock: Callable[[], float] = _agora,
    ):
        self.distracoes = [d for d in distracoes if d.strip()]
        self._clock = clock
        self._fim: float | None = None
        self._perguntou = False

    def iniciar(self, minutos: int) -> dict[str, Any]:
        if not 5 <= minutos <= 240:
            raise VidaError("o bloco de foco vai de 5 a 240 minutos")
        self._fim = self._clock() + minutos * 60
        self._perguntou = False
        return {"fim": self._fim}

    def ativo(self) -> bool:
        return self._fim is not None and self._clock() < self._fim

    def pergunta_permitida(self, alvo: str) -> bool:
        if not self.ativo() or self._perguntou or not e_distracao(alvo, self.distracoes):
            return False
        self._perguntou = True
        return True


class EstadoVoz:
    """E10.11: ritmo e energia da fala, comparados com a sua média. Só médias, em memória
    (regra 63): nada é gravado, nada decide, só muda o estilo da resposta."""

    def __init__(self) -> None:
        self._n = 0
        self._ritmo = 0.0
        self._energia = 0.0

    def observar(self, palavras: int, duracao_s: float, energia: float) -> str:
        if duracao_s <= 0 or palavras <= 0:
            return "normal"
        ritmo = palavras / (duracao_s / 60)
        estilo = "normal"
        if self._n >= 5:
            media_r = self._ritmo / self._n
            media_e = self._energia / self._n
            if ritmo > media_r * 1.25 or energia < media_e * 0.7:
                estilo = "curto"
        self._n += 1
        self._ritmo += ritmo
        self._energia += energia
        return estilo


class VidaJobs:
    """Passos automáticos de E10 (chamados pelo `JobRunner`): avisos de conta, diário e revisão.
    Tudo idempotente por dia (ou semana): rodar de novo não duplica nota nem aviso."""

    def __init__(
        self,
        memory: Any,
        financas: Financas,
        *,
        ops: Any,
        vault_dir: Path | None = None,
        repos: Iterable[Path] = (),
        diary_at: str = "",
        weekly_at: str = "",
        clock: Callable[[], float] = _agora,
    ) -> None:
        self.memory, self.financas, self._ops = memory, financas, ops
        self._vault, self._repos = vault_dir, list(repos)
        self._diary_at, self._weekly_at = diary_at, weekly_at
        self._clock = clock

    def tick(self) -> dict[str, Any]:
        agora = self._clock()
        dt = datetime.fromtimestamp(agora)
        out: dict[str, Any] = {
            "avisos": self.financas.avisos_de_contas(agora, self._notificar),
            "diario": None,
            "revisao": None,
        }
        if self._vault is None:
            return out
        hoje = int(dt.strftime("%Y%m%d"))
        if self._diary_at and _passou(dt, self._diary_at) and self._ultimo("diario") < hoje:
            out["diario"] = self._diario(dt.date())
            self.memory.counter_set("vida:diario", hoje)
        semana = int(dt.strftime("%G%V"))
        if (
            self._weekly_at
            and dt.weekday() == 6  # domingo
            and _passou(dt, self._weekly_at)
            and self._ultimo("revisao") < semana
        ):
            out["revisao"] = self._revisao(dt)
            self.memory.counter_set("vida:revisao", semana)
        return out

    def _ultimo(self, chave: str) -> int:
        return self.memory.counter_get(f"vida:{chave}")

    def _notificar(self, kind: str, text: str, ref: str) -> None:
        self._ops.notify(kind, text, ref=ref)

    def _diario(self, dia: date) -> str:
        assert self._vault is not None
        total, por = self.financas.gastos_do_dia(dia)
        texto = montar_diario(
            dia,
            commits=commits_do_dia(self._repos, dia),
            gastos_cents=total,
            gastos_por_categoria=por,
        )
        return str(escrever_no_vault(self._vault, "Diário", dia.isoformat(), texto))

    def _revisao(self, dt: datetime) -> str:
        assert self._vault is not None
        tarefas = [t["title"] for t in self._ops.list_tasks() if t["status"] != "concluida"]
        texto = montar_revisao(tarefas, projetos=[])
        return str(escrever_no_vault(self._vault, "Revisão", f"{dt:%G-W%V}", texto))


def _passou(dt: datetime, hhmm: str) -> bool:
    h, m = (int(x) for x in hhmm.split(":"))
    return (dt.hour, dt.minute) >= (h, m)
