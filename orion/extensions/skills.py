"""Pacotes Agent Skills: frontmatter limitado, referências confinadas e zero execução."""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator
from yaml.tokens import AliasToken, AnchorToken, TagToken


class SkillError(ValueError):
    pass


class SkillHeader(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True, frozen=True)
    name: str = Field(min_length=1, max_length=64, pattern=r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
    description: str = Field(min_length=1, max_length=1024)
    license: str | None = Field(default=None, max_length=1024)
    compatibility: str | None = Field(default=None, max_length=500)
    metadata: dict[str, str] = Field(default_factory=dict, max_length=32)
    allowed_tools: str | None = Field(default=None, alias="allowed-tools", max_length=2048)

    @field_validator("description")
    @classmethod
    def meaningful_description(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("description vazia")
        return value


class HeaderLoader(yaml.SafeLoader):
    """Rejeita chaves repetidas em vez de sobrescrever silenciosamente."""

    def construct_mapping(self, node, deep=False):
        keys = [self.construct_object(key, deep=deep) for key, _ in node.value]
        if any(not isinstance(key, str) for key in keys) or len(set(keys)) != len(keys):
            raise SkillError("frontmatter_duplicate_or_invalid_key")
        return super().construct_mapping(node, deep=deep)


def safe_file(root: Path, relative: str, *, limit: int = 256000) -> Path:
    if "\\" in relative or ":" in relative or "\0" in relative:
        raise SkillError("reference_out_of_scope")
    parts = PurePosixPath(relative).parts
    if not parts or PurePosixPath(relative).is_absolute() or ".." in parts:
        raise SkillError("reference_out_of_scope")
    current = root
    if root.is_symlink():
        raise SkillError("symlink_forbidden")
    for part in parts:
        current = current / part
        if current.is_symlink():
            raise SkillError("symlink_forbidden")
    try:
        current.resolve().relative_to(root.resolve())
        if not current.is_file() or current.stat().st_size > limit:
            raise SkillError("file_missing_or_too_large")
    except (OSError, ValueError) as error:
        raise SkillError("reference_out_of_scope") from error
    return current


def metadata(root: Path, *, namespace: str, origin: str = "local") -> Skill:
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,47}", namespace):
        raise SkillError("namespace_invalid")
    path = safe_file(root, "SKILL.md")
    # Ler somente frontmatter para descoberta; nenhum corpo/script é carregado.
    lines = []
    try:
        with path.open("rb") as stream:
            if stream.readline(4096).strip() != b"---":
                raise SkillError("frontmatter_missing")
            count = 0
            while True:
                line = stream.readline(4096)
                count += len(line)
                if not line or count > 16000:
                    raise SkillError("frontmatter_unclosed_or_too_large")
                if line.strip() == b"---":
                    break
                lines.append(line)
            offset = stream.tell()
        header = b"".join(lines).decode("utf-8")
        tokens = list(yaml.scan(header))
        if len(tokens) > 512 or any(
            isinstance(t, (AliasToken, AnchorToken, TagToken)) for t in tokens
        ):
            raise SkillError("frontmatter_unsupported_yaml")
        data = yaml.load(header, Loader=HeaderLoader)  # noqa: S506 — subclasse de SafeLoader
        meta = SkillHeader.model_validate(data)
    except (UnicodeError, yaml.YAMLError, ValidationError, OSError) as error:
        raise SkillError("frontmatter_invalid") from error
    if meta.name != root.name:
        raise SkillError("name_directory_mismatch")
    return Skill(root, namespace, origin, meta, offset)


@dataclass(frozen=True)
class SkillBody:
    text: str
    references: tuple[str, ...]
    digest: str


@dataclass(frozen=True)
class Skill:
    root: Path
    namespace: str
    origin: str
    header: SkillHeader
    offset: int

    @property
    def id(self) -> str:
        return f"{self.namespace}:{self.header.name}"

    @property
    def version(self) -> str:
        return self.header.metadata.get("version", "unversioned")[:80]

    def summary(self) -> dict:
        return {
            "id": self.id,
            "name": self.header.name,
            "description": self.header.description,
            "origin": self.origin,
            "version": self.version,
            "license": self.header.license,
        }

    def load(self) -> SkillBody:
        # Revalidar cabeçalho se o diretório local mudar desde a descoberta.
        current = metadata(self.root, namespace=self.namespace, origin=self.origin)
        if current.header != self.header:
            raise SkillError("skill_changed")
        path = safe_file(self.root, "SKILL.md")
        with path.open("rb") as stream:
            stream.seek(current.offset)
            raw = stream.read(64001)
        if len(raw) > 64000:
            raise SkillError("skill_body_too_large")
        try:
            text = raw.decode("utf-8")
        except UnicodeError as error:
            raise SkillError("skill_encoding_invalid") from error
        references = []
        # Links Markdown de arquivo e referências usuais, sem carregá-las no contexto.
        links = re.findall(r"\[[^\]]*\]\(([^)]+)\)", text)
        links.extend(re.findall(r"(?:references|scripts|assets)/[^\s`\"<>]+", text))
        for link in links:
            target = unquote(link.strip().strip("<>").split("#", 1)[0])
            if not target:
                continue
            uri = urlsplit(target)
            if uri.scheme in {"http", "https"}:
                continue  # link informativo; não buscar rede implicitamente
            if uri.scheme or uri.query:
                raise SkillError("reference_out_of_scope")
            # Referências extraídas de prosa podem terminar com pontuação Markdown.
            target = target.rstrip(").,")
            safe_file(self.root, target)
            if target not in references:
                references.append(target)
        return SkillBody(text, tuple(references), hashlib.sha256(raw).hexdigest())

    def reference(self, relative: str, *, limit: int = 16000) -> str:
        # Mesmo guard usado por validação: não abrir caminhos absolutos/externos/symlinks.
        path = safe_file(self.root, relative, limit=limit)
        with path.open("rb") as stream:
            data = stream.read(limit + 1)
        if len(data) > limit:
            raise SkillError("reference_too_large")
        try:
            return data.decode("utf-8")
        except UnicodeError as error:
            raise SkillError("reference_encoding_invalid") from error


class SkillIndex:
    def __init__(self):
        self.skills: dict[str, Skill] = {}

    def add(self, skill: Skill) -> None:
        if skill.id in self.skills:
            raise SkillError("skill_name_collision")
        self.skills[skill.id] = skill

    def discover(self, directory: Path, *, namespace: str, origin: str = "local") -> None:
        roots = sorted(directory.iterdir())
        if len(roots) > 128:
            raise SkillError("too_many_skills")
        pending = [
            metadata(root, namespace=namespace, origin=origin)
            for root in roots
            if (root / "SKILL.md").exists()
        ]
        if any(skill.id in self.skills for skill in pending):
            raise SkillError("skill_name_collision")
        for skill in pending:
            self.add(skill)

    def summaries(self) -> list[dict]:
        return [skill.summary() for skill in self.skills.values()]
