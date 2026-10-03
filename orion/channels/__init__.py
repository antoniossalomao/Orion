"""Canais de entrada do Orion (fase 5): por onde o Antônio fala com o agente."""

from .telegram import TelegramChannel, TelegramError

__all__ = ["TelegramChannel", "TelegramError"]
