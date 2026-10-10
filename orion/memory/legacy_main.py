"""Linhagem antiga do esquema da `main` (v3–v11), reconhecida e convertida para o esquema atual.

A `main` e a evolução (projetos com contexto isolado, resultados versionados, extensões) numeraram
as versões 3 em diante de formas diferentes, então o número sozinho não diz de qual banco se
trata. Um banco da `main` tem a tabela `audit` já na v3; o da evolução só ganha `audit` na v17.

`converter` não mexe no banco no lugar: sobe o arquivo até a v11 da `main` (com as migrações
congeladas abaixo), guarda a cópia `<arquivo>.main-vN.bak`, cria um banco novo no esquema atual e
copia tudo, traduzindo o que mudou de forma:

- conversas: fixada → favorita; arquivada/apagada/guardada → arquivada (as mensagens ficam);
- projetos: ids numéricos viram ids hexadecimais de 32 caracteres (e as referências os seguem);
- biblioteca de resultados da `main` (`artifacts`) → `library_results`.
"""

from __future__ import annotations

import contextlib
import os
import shutil
import sqlite3
import uuid
from pathlib import Path

from .schema import DDL, SCHEMA_VERSION, _fts

# v3: uma linha por decisão da política (já redigida: ver `orion.policy.audit.redact`).
_DDL_V3 = """
CREATE TABLE audit (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    session_id TEXT,
    tool TEXT NOT NULL,
    action TEXT NOT NULL,
    risk TEXT,
    reason TEXT NOT NULL DEFAULT '',
    tainted INTEGER NOT NULL DEFAULT 0,
    args TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_audit_ts ON audit(ts);
CREATE INDEX idx_audit_tool ON audit(tool, ts);
"""

_DDL_V4 = """
ALTER TABLE sessions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
"""

# A seleção não depende da última mensagem: uma resposta atrasada não troca a conversa.
_DDL_V5 = """
CREATE TABLE active_sessions (
    channel TEXT PRIMARY KEY,
    session_id TEXT UNIQUE REFERENCES sessions(id) ON DELETE SET NULL
);
INSERT INTO active_sessions(channel, session_id)
SELECT s.channel, s.id FROM sessions s
WHERE s.archived=0 AND s.deleted=0 AND s.id=(
    SELECT candidate.id FROM sessions candidate
    WHERE candidate.channel=s.channel AND candidate.archived=0 AND candidate.deleted=0
    ORDER BY candidate.last_active_at DESC, candidate.created_at DESC, candidate.id DESC LIMIT 1
);
"""

# Arquivar pelo usuário: sai da barra, mantém as mensagens (`archived` = importada, só leitura).
_DDL_V6 = """
ALTER TABLE sessions ADD COLUMN shelved INTEGER NOT NULL DEFAULT 0;
"""

# Projetos: contexto explícito (instruções próprias) para um grupo de conversas.
_DDL_V7 = """
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    instructions TEXT NOT NULL DEFAULT '',
    archived INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);
ALTER TABLE sessions ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL;
CREATE INDEX idx_sessions_project ON sessions(project_id);
"""

# Memória da tela (D3): SÓ texto lido por OCR local, com retenção curta; imagem nunca é guardada.
_DDL_V8 = f"""
CREATE TABLE screen_log (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL
);
CREATE INDEX idx_screen_ts ON screen_log(ts);
{_fts("screen_log")}
"""

# Documento de projeto (C32/C38): só aparece no contexto automático das conversas do projeto.
_DDL_V9 = """
ALTER TABLE documents ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL;
"""

# Biblioteca de resultados (C34): cópia do que o Orion gerou, com origem e versões.
_DDL_V10 = """
CREATE TABLE artifacts (
    id INTEGER PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,            -- documento | imagem
    name TEXT NOT NULL,
    stored TEXT NOT NULL,          -- nome do arquivo dentro da pasta da biblioteca
    bytes INTEGER NOT NULL,
    tool TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    parent_id INTEGER REFERENCES artifacts(id) ON DELETE SET NULL,
    created_at REAL NOT NULL
);
CREATE INDEX idx_artifacts_name ON artifacts(name, version);
CREATE INDEX idx_artifacts_created ON artifacts(created_at);
"""

# Registro de saída (regra 47): uma linha por chamada a provedor de fora, sem o conteúdo. Base da
# cota gratuita (regra 46), da telemetria por provedor e do painel de privacidade. `urgent`: o não
# perturbe (regra 48) segura na fila o aviso que não for urgente.
_DDL_V11 = """
CREATE TABLE external_calls (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    provider TEXT NOT NULL,      -- gateway:<camada>, groq, gemini, brave, ollama, cli:<nome>...
    kind TEXT NOT NULL,          -- chat, embed, transcribe, vision, search, tts, image, cli...
    model TEXT NOT NULL DEFAULT '',
    ok INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    bytes_out INTEGER NOT NULL DEFAULT 0,
    bytes_in INTEGER NOT NULL DEFAULT 0,
    content_kind TEXT NOT NULL DEFAULT ''   -- texto, imagem, audio (nunca o conteúdo)
);
CREATE INDEX idx_external_calls_ts ON external_calls(ts);
CREATE INDEX idx_external_calls_provider ON external_calls(provider, ts);
ALTER TABLE notifications ADD COLUMN urgent INTEGER NOT NULL DEFAULT 0;
"""


# versão de origem -> script que leva à seguinte (a v2 é comum às duas linhagens)
MAIN_MIGRATIONS: dict[int, str] = {
    2: _DDL_V3,
    3: _DDL_V4,
    4: _DDL_V5,
    5: _DDL_V6,
    6: _DDL_V7,
    7: _DDL_V8,
    8: _DDL_V9,
    9: _DDL_V10,
    10: _DDL_V11,
}
MAIN_VERSION = 11


def versao_da_main(conn: sqlite3.Connection) -> int | None:
    """A versão, se o banco é da linhagem antiga da `main` (v3–v11, com `audit`); senão None."""
    try:
        row = conn.execute("SELECT value FROM meta WHERE key='schema_version'").fetchone()
        tem_audit = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='audit'"
        ).fetchone()
    except sqlite3.DatabaseError:
        return None
    if row is None or not tem_audit:
        return None
    versao = int(row[0])
    return versao if 3 <= versao <= MAIN_VERSION else None


def _colunas(conn: sqlite3.Connection, esquema: str, tabela: str) -> list[str]:
    return [r[1] for r in conn.execute(f"PRAGMA {esquema}.table_info({tabela})")]


def _copiar(novo: sqlite3.Connection, tabela: str, origem: str | None = None) -> None:
    """Copia as colunas que existem nas duas pontas; o que o banco novo ganhou fica no padrão."""
    antigas = set(_colunas(novo, "old", origem or tabela))
    comuns = [c for c in _colunas(novo, "main", tabela) if c in antigas]
    lista = ", ".join(comuns)
    sql = f"INSERT INTO main.{tabela}({lista}) SELECT {lista} FROM old.{origem or tabela}"  # noqa: S608 — nomes fixos do esquema
    novo.execute(sql)


def converter(caminho: Path) -> int | None:
    """Converte o banco da `main` em `caminho` para o esquema atual. Devolve a versão de origem,
    ou None se o banco não é dessa linhagem (e nada foi tocado). A cópia fica em `.main-vN.bak`."""
    caminho = Path(caminho)
    with contextlib.closing(sqlite3.connect(caminho)) as c:
        versao = versao_da_main(c)
        if versao is None:
            return None
        c.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    backup = caminho.with_name(f"{caminho.name}.main-v{versao}.bak")
    trabalho = caminho.with_name(f"{caminho.name}.convertendo")
    novo_arquivo = caminho.with_name(f"{caminho.name}.novo")
    for lixo in (trabalho, novo_arquivo):
        lixo.unlink(missing_ok=True)
    if not backup.exists():
        shutil.copy2(caminho, backup)
    shutil.copy2(caminho, trabalho)
    try:
        with contextlib.closing(sqlite3.connect(trabalho)) as w:
            for v in range(versao, MAIN_VERSION):
                w.executescript(
                    f"BEGIN;{MAIN_MIGRATIONS[v]}"  # noqa: S608 — versão inteira vinda da tabela fixa
                    f"UPDATE meta SET value='{v + 1}' WHERE key='schema_version';COMMIT;"
                )
        with contextlib.closing(sqlite3.connect(novo_arquivo)) as n:
            n.executescript(DDL)
            n.execute("INSERT INTO meta VALUES ('schema_version', ?)", (str(SCHEMA_VERSION),))
            n.execute("PRAGMA foreign_keys=OFF")
            n.execute("ATTACH ? AS old", (str(trabalho),))
            n.execute("CREATE TEMP TABLE pmap(old INTEGER PRIMARY KEY, new TEXT NOT NULL)")
            for (antigo,) in n.execute("SELECT id FROM old.projects").fetchall():
                n.execute("INSERT INTO pmap VALUES (?, ?)", (antigo, uuid.uuid4().hex))
            n.execute(
                "INSERT INTO projects(id, name, instructions, archived, created_at, updated_at)"
                " SELECT m.new, o.name, o.instructions, o.archived, o.created_at, o.updated_at"
                " FROM old.projects o JOIN pmap m ON m.old=o.id"
            )
            n.execute(
                "INSERT INTO sessions(id, channel, title, created_at, last_active_at, archived,"
                " favorite, project_id)"
                " SELECT s.id, s.channel, s.title, s.created_at, s.last_active_at,"
                " CASE WHEN s.archived OR s.deleted OR s.shelved THEN 1 ELSE 0 END, s.pinned,"
                " (SELECT new FROM pmap WHERE old=s.project_id) FROM old.sessions s"
            )
            n.execute(
                "INSERT INTO documents(id, source, title, content_hash, indexed_at, project_id)"
                " SELECT d.id, d.source, d.title, d.content_hash, d.indexed_at,"
                " (SELECT new FROM pmap WHERE old=d.project_id) FROM old.documents d"
            )
            n.execute(
                "INSERT INTO library_results(id, session_id, project_id, kind, name, stored,"
                " bytes, tool, version, parent_id, created_at)"
                " SELECT a.id, a.session_id, (SELECT new FROM pmap WHERE old=a.project_id),"
                " a.kind, a.name, a.stored, a.bytes, a.tool, a.version, a.parent_id, a.created_at"
                " FROM old.artifacts a"
            )
            n.execute(
                "INSERT INTO meta(key, value) SELECT key, value FROM old.meta"
                " WHERE key<>'schema_version'"
            )
            for tabela in (
                "imported", "vectors", "messages", "facts", "chunks", "reminders", "schedules",
                "tasks", "numbers", "prompts", "edges", "notifications", "audit",
                "active_sessions", "screen_log", "external_calls",
            ):  # fmt: skip
                _copiar(n, tabela)
            problemas = n.execute("PRAGMA foreign_key_check").fetchall()
            if problemas:
                raise RuntimeError(
                    f"conversão do banco deixou referências quebradas: {problemas[:3]}"
                )
            n.commit()
            n.execute("DETACH old")
        for sufixo in ("-wal", "-shm"):
            Path(str(caminho) + sufixo).unlink(missing_ok=True)
        os.replace(novo_arquivo, caminho)
    finally:
        for lixo in (trabalho, novo_arquivo):
            lixo.unlink(missing_ok=True)
    return versao
