"""Câmera (E11.1, regra 64): uma foto por pedido, em memória, sem vídeo e sem arquivo.

A biblioteca (`opencv-python-headless`, extra `camera`) só é carregada quando a câmera é usada. A
foto volta como JPEG em bytes; quem chama a descarta logo depois da resposta. Nenhuma mensagem de
erro leva o índice do dispositivo nem caminho do sistema.
"""

from __future__ import annotations

from collections.abc import Callable

MAX_FOTO = 8 * 1024 * 1024  # bytes; uma foto de webcam passa disso só em caso de defeito
DESCARTAR_INICIAIS = 5  # webcams devolvem os primeiros quadros escuros: descarta alguns


class CameraError(RuntimeError):
    """Falha ao tirar a foto. A mensagem é curta e não tem caminho nem índice."""


Fonte = Callable[[int], bytes]


def foto_jpeg(device: int = 0, *, fonte: Fonte | None = None) -> bytes:
    """Um JPEG da câmera `device`. `fonte` troca a câmera real (testes e reuso)."""
    dados = fonte(device) if fonte is not None else _cv2_foto(device)
    if not dados:
        raise CameraError("a câmera não devolveu imagem")
    if len(dados) > MAX_FOTO:
        raise CameraError("a foto passou do tamanho máximo")
    return dados


def _cv2_foto(device: int) -> bytes:
    try:
        import cv2  # type: ignore[import-not-found]
    except ImportError:
        raise CameraError("instale o extra 'camera' (opencv-python-headless)") from None
    cap = cv2.VideoCapture(device)
    try:
        if not cap.isOpened():
            raise CameraError("não consegui abrir a câmera")
        for _ in range(DESCARTAR_INICIAIS):
            cap.read()
        ok, quadro = cap.read()
    finally:
        cap.release()
    if not ok or quadro is None:
        raise CameraError("a câmera não entregou quadro")
    ok, buf = cv2.imencode(".jpg", quadro, [cv2.IMWRITE_JPEG_QUALITY, 85])
    if not ok:
        raise CameraError("não consegui codificar a foto")
    return bytes(buf.tobytes())


def decodificar(jpeg: bytes):  # type: ignore[no-untyped-def]
    """JPEG → imagem BGR (ndarray) para os detectores. Exige o extra 'camera'."""
    try:
        import cv2  # type: ignore[import-not-found]
        import numpy as np
    except ImportError:
        raise CameraError("instale o extra 'camera' (opencv-python-headless)") from None
    imagem = cv2.imdecode(np.frombuffer(jpeg, dtype=np.uint8), cv2.IMREAD_COLOR)
    if imagem is None:
        raise CameraError("foto ilegível")
    return imagem
