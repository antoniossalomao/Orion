"""Configuração do Orion (pydantic-settings): variáveis `ORION_*` e `.env`."""

from __future__ import annotations

from pathlib import Path

from platformdirs import user_data_dir
from pydantic import Field, field_validator, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

PROJECT_ROOT = Path(__file__).resolve().parent.parent
_PUBLICOS = {"0.0.0.0", "::", ""}  # noqa: S104 — só para recusar


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="ORION_", env_file=".env", extra="ignore")

    host: str = "127.0.0.1"
    port: int = Field(default=8000, ge=1, le=65535)
    allow_public_bind: bool = False  # acesso de fora só pelo Tailscale (ver ORION_REGRAS.md)
    data_dir: Path = Field(default_factory=lambda: Path(user_data_dir("orion", appauthor=False)))
    log_level: str = "INFO"
    log_json: bool = True
    approval_ttl_s: int = Field(default=600, ge=30)
    extra_safe_roots: list[Path] = Field(default_factory=list)  # ex.: Documents no OneDrive
    admin_token: str = ""  # decide aprovações até o login da fase 5; vazio = desligado
    allowed_hosts: list[str] = Field(
        default_factory=lambda: ["127.0.0.1", "localhost"]
    )  # + Tailscale

    @field_validator("log_level")
    @classmethod
    def _nivel(cls, v: str) -> str:
        v = v.upper()
        if v not in {"DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"}:
            raise ValueError(f"log_level inválido: {v}")
        return v

    @model_validator(mode="after")
    def _sem_bind_publico(self) -> Settings:
        if self.host in _PUBLICOS and not self.allow_public_bind:
            raise ValueError(
                "host público recusado: o Orion só escuta em 127.0.0.1 "
                "(defina ORION_ALLOW_PUBLIC_BIND=true só se souber o que faz)"
            )
        return self

    @property
    def db_path(self) -> Path:
        return self.data_dir / "orion.db"
