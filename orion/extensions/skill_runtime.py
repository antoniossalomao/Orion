"""Seleção progressiva de skills: origem por turno e escopo somente restritivo."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field, field_validator

from ..tools.registry import ToolRegistry
from .context import ExternalData
from .skills import SkillError, SkillIndex


class SkillSource(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    root: Path
    namespace: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    origin: str = Field(default="local", max_length=128)
    enabled: bool = False

    @field_validator("root")
    @classmethod
    def absolute_root(cls, root: Path) -> Path:
        if not root.is_absolute():
            raise ValueError("raiz de skills precisa ser absoluta")
        return root


class SkillReference(BaseModel):
    model_config = ConfigDict(extra="forbid")
    skill: str = Field(max_length=120)
    path: str = Field(min_length=1, max_length=1024)


@dataclass(frozen=True)
class Selection:
    data: tuple[ExternalData, ...] = ()
    allowed_tools: frozenset[str] | None = None
    skills: tuple[dict, ...] = ()


class SkillRuntime:
    def __init__(self, sources: list[SkillSource]):
        self.index = SkillIndex()
        self.enabled: set[str] = set()
        self.diagnostics: list[dict] = []
        for source in sources:
            before = set(self.index.skills)
            try:
                self.index.discover(source.root, namespace=source.namespace, origin=source.origin)
                if source.enabled:
                    self.enabled.update(set(self.index.skills) - before)
            except (SkillError, OSError):
                self.diagnostics.append({"namespace": source.namespace, "code": "source_invalid"})

    def summaries(self) -> list[dict]:
        return [{**s, "enabled": s["id"] in self.enabled} for s in self.index.summaries()]

    def select(
        self,
        query: str,
        explicit: list[str] | None = None,
        references: list[SkillReference] | None = None,
        *,
        budget: int = 12000,
    ) -> Selection:
        ids = list(dict.fromkeys(explicit or []))
        if len(ids) > 3 or len(references or []) > 8:
            raise SkillError("skill_selection_too_many")
        if not ids:
            words = ToolRegistry._words(query)
            ranked = []
            for id_ in self.enabled:
                skill = self.index.skills[id_]
                score = len(words & ToolRegistry._words(skill.header.description))
                if score >= 2:
                    ranked.append((score, id_))
            ranked.sort(key=lambda row: (-row[0], row[1]))
            ids = [id_ for _, id_ in ranked[:3]]
        data, provenance = [], []
        scope: frozenset[str] | None = None
        loaded = {}
        remaining = max(0, min(budget, 24000))
        for id_ in ids:
            skill = self.index.skills.get(id_)
            if skill is None:
                raise SkillError("skill_not_found")
            if id_ not in self.enabled:
                raise SkillError("skill_disabled")
            body = skill.load()
            loaded[id_] = body
            raw = body.text.encode()
            text = raw[:remaining].decode(errors="ignore")
            remaining -= len(text.encode())
            data.append(
                ExternalData(
                    skill.namespace, "skill", id_, text, body.digest, len(raw) > len(text.encode())
                )
            )
            provenance.append({**skill.summary(), "digest": body.digest})
            if skill.header.allowed_tools is not None:
                restriction = frozenset(skill.header.allowed_tools.split())
                scope = restriction if scope is None else scope & restriction
        for reference in references or []:
            if reference.skill not in loaded:
                raise SkillError("skill_reference_not_selected")
            body = loaded[reference.skill]
            if reference.path not in body.references:
                raise SkillError("skill_reference_not_declared")
            skill = self.index.skills[reference.skill]
            raw = skill.reference(reference.path).encode()
            text = raw[:remaining].decode(errors="ignore")
            remaining -= len(text.encode())
            data.append(
                ExternalData(
                    skill.namespace,
                    "skill_reference",
                    reference.path,
                    text,
                    hashlib.sha256(raw).hexdigest(),
                    len(raw) > len(text.encode()),
                )
            )
        return Selection(tuple(data), scope, tuple(provenance))
