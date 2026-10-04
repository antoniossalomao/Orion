"""Verificação da fase 0 (antes de vender o PC): o export abre, as contagens batem e a
memória nova consegue guardá-lo, fazer backup, restaurar e responder.

`verify_export` nunca toca no banco real: importa para um arquivo temporário. Também
confere, só por existência e tamanho, o que precisa ser copiado do PC (`.env`,
`google_auth/`, vault) — nunca lê nem imprime valores de segredo (só os NOMES das chaves).
"""

from __future__ import annotations

import json
import re
import tempfile
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path

from .memory import MemoryStore
from .memory.importer import OPERACIONAIS, RELACOES, ImportReport, import_surreal_export

OBRIGATORIAS = ("evento", "sessao")
_CHAVE_ENV = re.compile(r"^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\S")


@dataclass
class MigrationReport:
    tabelas: dict[str, int] = field(default_factory=dict)  # arquivo do export -> registros
    ausentes: list[str] = field(default_factory=list)  # esperadas e sem arquivo (aviso)
    erros: list[str] = field(default_factory=list)
    avisos: list[str] = field(default_factory=list)
    importacao: ImportReport | None = None
    ensaio: dict[str, int] = field(default_factory=dict)  # contagens do banco restaurado
    arquivos: dict[str, str] = field(default_factory=dict)  # o que copiar -> situação

    @property
    def ok(self) -> bool:
        return not self.erros

    def linhas(self) -> list[str]:
        out = ["EXPORT"]
        out += [f"  {t}: {n} registros" for t, n in sorted(self.tabelas.items())] or ["  (vazio)"]
        out += [f"  ausente (ok se o legado nunca usou): {t}" for t in self.ausentes]
        if self.importacao:
            out += ["IMPORTAÇÃO DE ENSAIO (banco temporário)", "  " + self.importacao.resumo()]
        if self.ensaio:
            out += ["BACKUP E RESTAURAÇÃO"] + [f"  {t}: {n}" for t, n in self.ensaio.items() if n]
        if self.arquivos:
            out += ["ARQUIVOS A COPIAR DO PC"] + [f"  {a}: {s}" for a, s in self.arquivos.items()]
        out += [f"AVISO: {a}" for a in self.avisos]
        out += [f"ERRO: {e}" for e in self.erros]
        out.append("RESULTADO: " + ("PRONTO" if self.ok else "NÃO está pronto"))
        return out


def _contar(pasta: Path, rel: MigrationReport) -> dict[str, list[dict]]:
    dados: dict[str, list[dict]] = {}
    esperadas = (*OBRIGATORIAS, *OPERACIONAIS, *RELACOES)
    com_problema: set[str] = set()
    for arq in sorted(pasta.glob("*.json")):
        try:
            conteudo = json.loads(arq.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            rel.erros.append(f"{arq.name} ilegível: {e}")
            com_problema.add(arq.stem)
            continue
        if not isinstance(conteudo, list):
            rel.erros.append(f"{arq.name}: esperava uma lista de registros")
            com_problema.add(arq.stem)
            continue
        dados[arq.stem] = [d for d in conteudo if isinstance(d, dict)]
        rel.tabelas[arq.stem] = len(conteudo)
        if len(dados[arq.stem]) != len(conteudo):
            rel.erros.append(f"{arq.name}: há itens que não são registros")
    for t in esperadas:
        if t in rel.tabelas or t in com_problema:
            continue
        if t in OBRIGATORIAS:
            rel.erros.append(f"{t}.json não existe (rode backup_memoria no legado)")
        else:
            rel.ausentes.append(t)
    if rel.tabelas.get("evento") == 0:
        rel.erros.append("evento.json está vazio: o export não tem conversas")
    return dados


def _conferir_contas(imp: ImportReport, dados: dict[str, list[dict]], rel: MigrationReport) -> None:
    """Todo registro do export tem de estar contado: novo, repetido ou inválido (e dito qual)."""

    def conta(tabela: str, novos: int, repetidos: int) -> None:
        total = len(dados.get(tabela, []))
        contados = novos + repetidos + imp.invalidas_por_tabela.get(tabela, 0)
        if total != contados:
            rel.erros.append(
                f"{tabela}: {total} no export, {contados} contados na importação "
                f"(novos {novos}, repetidos {repetidos}, inválidos "
                f"{imp.invalidas_por_tabela.get(tabela, 0)})"
            )

    # sessões criadas no ensaio incluem as "órfãs" que os eventos pedem: só o piso é conferível
    validas = len(dados.get("sessao", [])) - imp.invalidas_por_tabela.get("sessao", 0)
    if imp.sessoes < validas:
        rel.erros.append(f"sessao: {validas} válidas no export, só {imp.sessoes} importadas")
    conta("evento", imp.mensagens, imp.ja_importadas)
    for t in OPERACIONAIS:
        conta(t, imp.operacionais.get(t, 0), imp.operacionais_repetidos.get(t, 0))
    for t, n in imp.invalidas_por_tabela.items():
        rel.avisos.append(
            f"{n} registro(s) inválido(s) em {t}.json (ver o log; não são importados)"
        )
    if imp.atores_como_system:
        quem = ", ".join(f"{a} ({n})" for a, n in sorted(imp.atores_como_system.items()))
        rel.avisos.append(
            f"atores que viram 'system': {quem}. Se algum é o nome antigo do assistente, passe "
            "--assistente <nome> (senão essas falas não aparecem como respostas na busca)"
        )


def _ensaio(pasta: Path, assistentes: Iterable[str], rel: MigrationReport, dados) -> None:
    with tempfile.TemporaryDirectory(prefix="orion-verify-") as tmp:
        tmp_dir = Path(tmp)
        store = MemoryStore(tmp_dir / "ensaio.db")
        try:
            rel.importacao = imp = import_surreal_export(store, pasta, assistentes)
            _conferir_contas(imp, dados, rel)
            esperadas = store.query("SELECT COUNT(*) FROM messages")[0][0]
            amostra = store.query(
                "SELECT text FROM messages WHERE role IN ('user','assistant') "
                "ORDER BY length(text) DESC LIMIT 1"
            )
            backup = store.backup_to(tmp_dir / "bk" / "orion.db")
        finally:
            store.close()
        rel.ensaio = MemoryStore.verify_backup(backup)
        restaurado = MemoryStore.restore(backup, tmp_dir / "novo" / "orion.db")
        try:
            if restaurado.query("SELECT COUNT(*) FROM messages")[0][0] != esperadas:
                rel.erros.append("a restauração do backup perdeu mensagens")
            if amostra:
                trecho = " ".join(amostra[0][0].split()[:12])
                if not restaurado.search(trecho, k=3, kinds=["message"]):
                    rel.erros.append("a busca não achou, no banco restaurado, uma fala importada")
        finally:
            restaurado.close()


def _conferir_arquivos(
    rel: MigrationReport, env_file: Path | None, google_auth: Path | None, vault: Path | None
) -> None:
    if env_file is not None:
        if not env_file.is_file() or env_file.stat().st_size == 0:
            rel.erros.append(f".env não encontrado ou vazio: {env_file}")
        else:
            chaves = sorted(
                {
                    m.group(1)
                    for ln in env_file.read_text(encoding="utf-8", errors="replace").splitlines()
                    if (m := _CHAVE_ENV.match(ln)) and not ln.lstrip().startswith("#")
                }
            )
            rel.arquivos[".env"] = f"{len(chaves)} variáveis preenchidas ({', '.join(chaves)})"
    if google_auth is not None:
        arqs = [a for a in google_auth.glob("*") if a.is_file() and a.stat().st_size > 0]
        if not arqs:
            rel.erros.append(f"google_auth sem arquivos (ou vazios): {google_auth}")
        else:
            rel.arquivos["google_auth/"] = f"{len(arqs)} arquivo(s): " + ", ".join(
                sorted(a.name for a in arqs)
            )
    if vault is not None:
        notas = (
            [
                n
                for n in vault.rglob("*.md")
                if not any(p.startswith(".") for p in n.relative_to(vault).parts)
            ]
            if vault.is_dir()
            else []
        )
        if not notas:
            rel.erros.append(f"vault sem notas .md: {vault}")
        else:
            rel.arquivos["vault"] = f"{len(notas)} notas .md"


def verify_export(
    pasta: Path | str,
    *,
    assistentes: Iterable[str] = ("Orion",),
    env_file: Path | None = None,
    google_auth: Path | None = None,
    vault: Path | None = None,
    ensaio: bool = True,
) -> MigrationReport:
    pasta = Path(pasta)
    rel = MigrationReport()
    if not pasta.is_dir():
        rel.erros.append(f"pasta do export não existe: {pasta}")
        return rel
    dados = _contar(pasta, rel)
    if ensaio and rel.ok:
        _ensaio(pasta, assistentes, rel, dados)
    _conferir_arquivos(rel, env_file, google_auth, vault)
    return rel
