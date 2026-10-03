"""Logging estruturado (JSON por linha) com request-id e redação de segredos."""

from __future__ import annotations

import contextvars
import json
import logging
import sys
from datetime import UTC, datetime

from .policy.audit import redact

request_id: contextvars.ContextVar[str] = contextvars.ContextVar("request_id", default="-")

_RESERVADOS = set(logging.makeLogRecord({}).__dict__) | {"message", "asctime"}


class JsonFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        dados: dict[str, object] = {
            "ts": datetime.fromtimestamp(record.created, UTC).isoformat(timespec="milliseconds"),
            "level": record.levelname,
            "logger": record.name,
            "msg": redact(record.getMessage(), limite=2000),
            "request_id": request_id.get(),
        }
        extras = {k: v for k, v in record.__dict__.items() if k not in _RESERVADOS}
        if extras:
            dados.update(redact(extras))
        if record.exc_info:
            dados["exc"] = self.formatException(record.exc_info)
        return json.dumps(dados, ensure_ascii=False, default=str)


def setup_logging(level: str = "INFO", json_logs: bool = True) -> None:
    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(
        JsonFormatter()
        if json_logs
        else logging.Formatter("%(asctime)s %(levelname)s %(name)s [%(request_id)s] %(message)s")
    )
    if not json_logs:
        handler.addFilter(_InjetaRequestId())
    raiz = logging.getLogger()
    raiz.handlers[:] = [handler]
    raiz.setLevel(level)
    # o httpx registra "POST https://api.telegram.org/bot<TOKEN>/..." em INFO: nunca vai ao log
    for nome in ("httpx", "httpcore"):
        logging.getLogger(nome).setLevel(max(logging.WARNING, raiz.level))


class _InjetaRequestId(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        record.request_id = request_id.get()
        return True
