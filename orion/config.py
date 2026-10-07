"""Configuração do Orion (pydantic-settings): variáveis `ORION_*` e `.env`."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Annotated

from platformdirs import user_data_dir
from pydantic import Field, field_validator, model_validator
from pydantic_settings import BaseSettings, NoDecode, SettingsConfigDict

from .extensions.host import ConnectionConfig
from .extensions.skill_runtime import SkillSource

PROJECT_ROOT = Path(__file__).resolve().parent.parent
_PUBLICOS = {"0.0.0.0", "::", ""}  # noqa: S104 — só para recusar
_LOCAIS = {"127.0.0.1", "localhost", "::1"}


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
    serve_ui: bool = True  # serve a interface em /ui/ (mesma origem, sem CORS)
    admin_token: str = ""  # decide aprovações até o login da fase 5; vazio = desligado
    # Gateway de modelos (OmniRoute local ou qualquer API compatível com a da OpenAI).
    gateway_url: str = ""  # ex.: http://127.0.0.1:20128/v1 — vazio: /chat desligado
    gateway_model: str = ""
    gateway_api_key: str = ""  # ou no cofre do SO (orion.secrets)
    allowed_hosts: list[str] = Field(
        default_factory=lambda: ["127.0.0.1", "localhost"]
    )  # + Tailscale (só com admin_token: ver `_acesso_de_fora_exige_token`)
    # `orion-desktop` v0 (executar_comando, ler_arquivo, listar_arquivos): desligado por padrão
    skill_sources: list[SkillSource] = Field(default_factory=list, max_length=32)
    mcp_connections: list[ConnectionConfig] = Field(default_factory=list, max_length=32)
    desktop_tools: bool = False
    # Canal Telegram (fase 5): sobe se houver token; sem lista de usuários não sobe (default-deny).
    telegram_token: str = ""  # ou no cofre do SO (ORION_TELEGRAM_TOKEN)
    # IDs numéricos do Telegram, separados por vírgula ("123,456") ou lista JSON ("[123]")
    telegram_allowed_users: Annotated[list[int], NoDecode] = Field(default_factory=list)
    # Jobs em segundo plano (lembretes, agendamentos, embeddings, vault, backup, consolidação).
    jobs_enabled: bool = True
    jobs_tick_s: float = Field(default=30.0, ge=1.0)
    backup_dir: Path | None = None  # padrão: <dados>/backups; aponte para o iCloud/OneDrive
    backup_keep: int = Field(default=7, ge=1)
    vault_dir: Path | None = None  # vault do Obsidian a indexar na memória (vazio: não indexa)
    consolidate: bool = True  # fatos a partir das conversas (precisa do gateway)
    # Embeddings por API gratuita (Gemini). Sem chave, a busca é só por palavra-chave.
    embed_api_key: str = ""  # ou no cofre do SO (ORION_EMBED_API_KEY)
    embed_model: str = "gemini-embedding-001"
    embed_dim: int = Field(default=768, ge=64, le=3072)

    @field_validator("admin_token")
    @classmethod
    def _token_forte(cls, v: str) -> str:
        if v and len(v) < 16:
            raise ValueError(
                "ORION_ADMIN_TOKEN precisa de 16+ caracteres "
                '(gere com: python -c "import secrets;print(secrets.token_urlsafe(32))")'
            )
        return v

    @field_validator("telegram_allowed_users", mode="before")
    @classmethod
    def _ids_do_telegram(cls, v: object) -> object:
        if isinstance(v, str):
            v = v.strip()
            if not v:
                return []
            return (
                json.loads(v) if v.startswith("[") else [int(x) for x in v.split(",") if x.strip()]
            )
        return v

    @field_validator("backup_dir", "vault_dir", mode="before")
    @classmethod
    def _vazio_e_nao_definido(cls, v: object) -> object:
        """`ORION_VAULT_DIR=` (vazio) viraria `Path('.')`: indexaria/gravaria na pasta atual."""
        return None if isinstance(v, str) and not v.strip() else v

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

    @model_validator(mode="after")
    def _acesso_de_fora_exige_token(self) -> Settings:
        """Tailscale (ou qualquer host que não seja local) só depois de haver login: sem token
        as rotas que mudam estado ficam trancadas, mas a exposição em si já é recusada."""
        de_fora = [h for h in self.allowed_hosts if h not in _LOCAIS]
        if de_fora and not self.admin_token:
            raise ValueError(
                f"host {', '.join(de_fora)} em ORION_ALLOWED_HOSTS exige ORION_ADMIN_TOKEN "
                "(regra 17 do ORION_REGRAS.md: acesso de fora só com login)"
            )
        return self

    @model_validator(mode="after")
    def _telegram_nao_sobe_aberto(self) -> Settings:
        if self.telegram_token and not self.telegram_allowed_users:
            raise ValueError(
                "ORION_TELEGRAM_TOKEN exige ORION_TELEGRAM_ALLOWED_USERS: "
                "sem lista de usuários o bot não sobe (default-deny)"
            )
        return self

    @property
    def db_path(self) -> Path:
        return self.data_dir / "orion.db"

    @property
    def effective_backup_dir(self) -> Path:
        return self.backup_dir or self.data_dir / "backups"
