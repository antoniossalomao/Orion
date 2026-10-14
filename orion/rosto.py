"""Reconhecimento facial local (E11.5, regra 65).

Cadastro só do dono, pelo comando `orion rosto`; não há ferramenta do modelo para isso. Cada
vetor facial é cifrado (Fernet) antes de entrar no banco; a chave fica no cofre do sistema
(`ORION_FACE_KEY`). O rosto é recortado com o detector de faces que vem no OpenCV (Haar), e o
vetor sai do modelo ONNX local (`ORION_FACE_MODEL`). Nada disso sai do computador.
"""

from __future__ import annotations

import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

import numpy as np
from cryptography.fernet import Fernet, InvalidToken

from .secrets import delete_secret, get_secret, set_secret

CHAVE = "ORION_FACE_KEY"
LIMIAR = 0.45  # similaridade de cosseno a partir da qual é você
MIN_AMOSTRAS = 3  # fotos com rosto necessárias para cadastrar (são tiradas 5)
TAMANHO_ROSTO = 112  # entrada típica dos modelos de rosto (ArcFace)

Embedder = Callable[[np.ndarray], np.ndarray | None]


class RostoError(RuntimeError):
    """Erro que pode ser mostrado ao dono. Nunca leva vetor, caminho de chave nem imagem."""


def _fernet(get: Callable[[str], str | None], set_: Callable[[str, str], None]) -> Fernet:
    chave = get(CHAVE)
    if not chave:
        chave = Fernet.generate_key().decode()
        set_(CHAVE, chave)
    return Fernet(chave.encode())


class Rostos:
    def __init__(
        self,
        memory: Any,
        embedder: Embedder,
        *,
        get: Callable[[str], str | None] = get_secret,
        set_: Callable[[str, str], None] = set_secret,
    ) -> None:
        self.memory, self._embed, self._get, self._set = memory, embedder, get, set_

    def _vetor(self, bgr: np.ndarray) -> np.ndarray | None:
        v = self._embed(bgr)
        if v is None:
            return None
        v = np.asarray(v, dtype=np.float32).ravel()
        n = float(np.linalg.norm(v))
        return v / n if n > 0 else None

    def cadastrar(self, nome: str, fotos: list[np.ndarray]) -> int:
        nome = nome.strip()[:80]
        if not nome:
            raise RostoError("diga o nome do cadastro")
        vetores = [v for v in (self._vetor(f) for f in fotos) if v is not None]
        if len(vetores) < MIN_AMOSTRAS:
            raise RostoError(f"rosto não encontrado em pelo menos {MIN_AMOSTRAS} fotos")
        f = _fernet(self._get, self._set)
        with self.memory.transaction() as c:
            c.executemany(
                "INSERT INTO faces(name, embedding, created_at) VALUES (?,?,?)",
                [(nome, f.encrypt(v.tobytes()), time.time()) for v in vetores],
            )
        return len(vetores)

    def reconhecer(self, bgr: np.ndarray) -> bool:
        """True se a imagem tem um rosto cadastrado acima do limiar. Sem cadastro: False."""
        alvo = self._vetor(bgr)
        if alvo is None:
            return False
        linhas = self.memory.query("SELECT embedding FROM faces")
        if not linhas:
            return False
        f = _fernet(self._get, self._set)
        for r in linhas:
            try:
                ref = np.frombuffer(f.decrypt(bytes(r["embedding"])), dtype=np.float32)
            except InvalidToken:
                continue  # chave trocada: o vetor antigo não serve mais
            if ref.size and float(np.dot(alvo, ref / (np.linalg.norm(ref) or 1.0))) >= LIMIAR:
                return True
        return False

    def cadastrados(self) -> list[tuple[str, int]]:
        return [
            (r["name"], r["n"])
            for r in self.memory.query(
                "SELECT name, COUNT(*) AS n FROM faces GROUP BY name ORDER BY name"
            )
        ]

    def apagar(self) -> int:
        """Remove todos os vetores e a chave. Sem a chave, nada do que sobrou é legível."""
        with self.memory.transaction() as c:
            n = c.execute("DELETE FROM faces").rowcount
        try:
            delete_secret(CHAVE)
        except Exception:  # noqa: BLE001 — sem cofre, a chave já não existe para o Orion ler
            self._set(CHAVE, "")
        return n


def embedder_onnx(modelo: Path) -> Embedder:
    """Embedder pelo modelo ONNX local. Recorta o rosto com o Haar do OpenCV antes."""
    import cv2  # type: ignore[import-not-found]

    if not modelo.is_file():
        raise RostoError("modelo de rosto não encontrado (ORION_FACE_MODEL)")
    import onnxruntime  # type: ignore[import-not-found]

    sessao = onnxruntime.InferenceSession(str(modelo), providers=["CPUExecutionProvider"])
    entrada = sessao.get_inputs()[0].name
    detector = cv2.CascadeClassifier(  # type: ignore[attr-defined]
        str(Path(cv2.data.haarcascades) / "haarcascade_frontalface_default.xml")  # type: ignore[attr-defined]
    )

    def embed(bgr: np.ndarray) -> np.ndarray | None:
        cinza = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
        achados = detector.detectMultiScale(cinza, 1.1, 5, minSize=(60, 60))
        if len(achados) == 0:
            return None
        x, y, w, h = max(achados, key=lambda r: int(r[2]) * int(r[3]))  # o maior rosto
        recorte = cv2.resize(bgr[y : y + h, x : x + w], (TAMANHO_ROSTO, TAMANHO_ROSTO))
        x_in = recorte[:, :, ::-1].transpose(2, 0, 1)[None].astype(np.float32)
        x_in = (x_in - 127.5) / 127.5
        return np.asarray(sessao.run(None, {entrada: x_in})[0])

    return embed
