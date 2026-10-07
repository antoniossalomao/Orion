"""Configuração do Orion (pydantic-settings): variáveis `ORION_*` e `.env`."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Annotated

from platformdirs import user_data_dir
from pydantic import Field, field_validator, model_validator
from pydantic_settings import BaseSettings, NoDecode, SettingsConfigDict

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
    admin_token: str = ""  # credencial de máquina (curl, scripts); vazio = só o login com senha
    # Login com senha (`orion set-password`) e sessão por cookie httpOnly.
    auth_user: str = "admin"
    # Cria admin / senha de fábrica se ainda não há senha (o .exe liga isto). Ver orion/auth.py.
    seed_default_password: bool = False
    session_ttl_h: int = Field(default=168, ge=1, le=24 * 90)  # validade da sessão: 7 dias
    cookie_secure: bool = False  # true atrás de HTTPS (`tailscale serve`); em https é automático
    audit_retention_days: int = Field(default=90, ge=1)  # trilha de decisões da política
    # Gateway de modelos (OmniRoute local ou qualquer API compatível com a da OpenAI).
    gateway_url: str = ""  # ex.: http://127.0.0.1:20128/v1 — vazio: /chat desligado
    gateway_model: str = ""
    # Roteamento por tipo de tarefa (orion/router.py): modelo para conversa curta e para trabalho
    # pesado, no mesmo gateway. Vazios: tudo vai no `gateway_model`. A camada de imagem usa
    # `vision_model`. No OmniRoute, o "modelo" pode ser um combo seu (ex.: "rapido", "forte").
    gateway_model_fast: str = ""
    gateway_model_heavy: str = ""
    gateway_api_key: str = ""  # ou no cofre do SO (orion.secrets)
    allowed_hosts: list[str] = Field(
        default_factory=lambda: ["127.0.0.1", "localhost"]
    )  # + Tailscale: só sobe com senha (`orion set-password`) ou token de admin (regra 17)
    # `orion-desktop` v0 (executar_comando, ler_arquivo, listar_arquivos): desligado por padrão
    desktop_tools: bool = False
    # Ferramentas de web (buscar_url, consultar_clima, pesquisar_com_ia): desligadas por padrão,
    # porque página lida pode mandar o modelo buscar outra URL com dados na query (tools/web.py).
    web_tools: bool = False
    # Visão (capturar_tela, explicar_tela, analisar_imagem): precisa do desktop e de um modelo que
    # aceite imagem. Desligada por padrão: a tela mostra o que você está fazendo e `explicar_tela`
    # manda a imagem para o provedor do modelo.
    vision_tools: bool = False
    vision_model: str = ""  # vazio: o mesmo modelo do gateway
    weather_city: str = "Marília"  # cidade quando o pedido não diz qual
    search_api_key: str = ""  # Gemini com Google Search; sem ela vale a chave de embeddings
    search_model: str = "gemini-2.5-flash"
    brave_api_key: str = (
        ""  # `pesquisar_internet` (Brave Search); ou no cofre (ORION_BRAVE_API_KEY)
    )
    # `gerar_imagem`: modelo de imagem do Gemini. Chave: a daqui, ou a de busca/embeddings (mesmo
    # Google AI Studio). A imagem fica em <dados>/imagens e só é servida com login.
    image_model: str = "gemini-2.5-flash-image"
    image_api_key: str = ""
    # Canal Telegram (fase 5): sobe se houver token; sem lista de usuários não sobe (default-deny).
    telegram_token: str = ""  # ou no cofre do SO (ORION_TELEGRAM_TOKEN)
    # IDs numéricos do Telegram, separados por vírgula ("123,456") ou lista JSON ("[123]")
    telegram_allowed_users: Annotated[list[int], NoDecode] = Field(default_factory=list)
    # Servidores MCP (fase 4): sobem do mcp.json (padrão: <dados>/mcp.json), só com gateway.
    mcp_enabled: bool = True
    mcp_config: Path | None = None
    # Voz no Telegram: transcrição por API compatível com a da OpenAI (Whisper no Groq, grátis).
    transcribe_api_key: str = ""  # ou no cofre do SO (ORION_TRANSCRIBE_API_KEY); vazio: sem voz
    transcribe_url: str = "https://api.groq.com/openai/v1"
    transcribe_model: str = "whisper-large-v3-turbo"
    # Voz (fase 6). A: fala no navegador vira um turno do agente (precisa da chave de transcrição)
    # e a resposta é falada por edge-tts — o TEXTO da resposta vai para a Microsoft. Desligada.
    voice_enabled: bool = False
    voice_speak: bool = True  # false: só transcreve e responde em texto, sem sintetizar a fala
    voice_tts_voice: str = "pt-BR-AntonioNeural"
    # B: voz ao vivo (Gemini Live). O áudio do microfone vai para o Google; o modelo só conversa
    # (sem ferramentas nem memória). Desligada. Chave: a daqui ou a de busca/embeddings.
    voice_live_enabled: bool = False
    voice_live_model: str = "gemini-2.5-flash-native-audio-latest"
    voice_live_voice: str = "Charon"
    voice_live_api_key: str = ""
    voice_live_max_min: int = Field(default=20, ge=1, le=60)  # teto de uma sessão, em minutos
    # Jobs em segundo plano (lembretes, agendamentos, embeddings, vault, backup, consolidação).
    jobs_enabled: bool = True
    jobs_tick_s: float = Field(default=30.0, ge=1.0)
    backup_dir: Path | None = None  # padrão: <dados>/backups; aponte para o iCloud/OneDrive
    backup_keep: int = Field(default=7, ge=1)
    vault_dir: Path | None = None  # vault do Obsidian a indexar na memória (vazio: não indexa)
    capture_folder: str = "00 Inbox"  # /capturar (Telegram) grava aqui, dentro do vault
    briefing_at: str = ""  # HH:MM: aviso diário com lembretes, agendamentos e tarefas (vazio: não)
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

    @field_validator("backup_dir", "vault_dir", "mcp_config", mode="before")
    @classmethod
    def _vazio_e_nao_definido(cls, v: object) -> object:
        """`ORION_VAULT_DIR=` (vazio) viraria `Path('.')`: indexaria/gravaria na pasta atual."""
        return None if isinstance(v, str) and not v.strip() else v

    @field_validator("capture_folder")
    @classmethod
    def _pasta_de_captura(cls, v: str) -> str:
        v = v.strip()
        partes = Path(v.replace("\\", "/")).parts
        if not v or v[0] in "/\\~" or Path(v).is_absolute() or ".." in partes or ":" in v:
            raise ValueError(
                "ORION_CAPTURE_FOLDER precisa ser uma pasta relativa do vault (ex.: 00 Inbox)"
            )
        return v.rstrip("/\\")

    @field_validator("briefing_at")
    @classmethod
    def _hora_do_briefing(cls, v: str) -> str:
        v = v.strip()
        if not v:
            return ""
        if not re.fullmatch(r"([01]\d|2[0-3]):[0-5]\d", v):
            raise ValueError("ORION_BRIEFING_AT precisa estar no formato HH:MM (ex.: 07:30)")
        return v

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
    def _telegram_nao_sobe_aberto(self) -> Settings:
        if self.telegram_token and not self.telegram_allowed_users:
            raise ValueError(
                "ORION_TELEGRAM_TOKEN exige ORION_TELEGRAM_ALLOWED_USERS: "
                "sem lista de usuários o bot não sobe (default-deny)"
            )
        return self

    @property
    def hosts_de_fora(self) -> list[str]:
        """Hosts permitidos que não são locais (Tailscale, curinga): só com login (regra 17)."""
        return [h for h in self.allowed_hosts if h not in _LOCAIS]

    @property
    def db_path(self) -> Path:
        return self.data_dir / "orion.db"

    @property
    def auth_db_path(self) -> Path:
        """Fora do banco da memória: o backup vai para a nuvem e não leva o hash da senha."""
        return self.data_dir / "auth.db"

    @property
    def effective_mcp_config(self) -> Path:
        return self.mcp_config or self.data_dir / "mcp.json"

    @property
    def effective_backup_dir(self) -> Path:
        return self.backup_dir or self.data_dir / "backups"
