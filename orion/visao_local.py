"""Detecção local de objetos (E11.3) e memória de avistamentos (E11.4), regra 64.

O detector é um YOLO nano exportado para ONNX (`ORION_OBJECT_MODEL`), rodando na CPU com
`onnxruntime`. O arquivo do modelo é colocado por você: o Orion não baixa modelo sozinho (download
seria tráfego de rede sem pedido seu). Sem o arquivo, a detecção simplesmente não roda.

Os avistamentos guardam **só o rótulo**, a confiança e a posição do centro. Nunca a imagem, nunca a
classe "person" (regra 64) e apagam sozinhos depois de 7 dias. Os rótulos são os do COCO (inglês);
os nomes em português que o modelo pede passam por `ALIAS`. Objeto que o COCO não tem (fone de
ouvido, por exemplo) não é detectado: essa é a limitação do modelo, não do código.
"""

from __future__ import annotations

import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

ENTRADA = 640  # lado da entrada do YOLO
LIMIAR = 0.35  # confiança mínima
IOU_MAX = 0.45  # sobreposição acima disso = mesma caixa (NMS)
RETENCAO_S = 7 * 24 * 3600
NUNCA_GUARDAR = frozenset({"person"})  # regra 64: pessoa não entra na memória

# Nomes do COCO (80 classes, na ordem do modelo)
COCO_NOMES = [
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat",
    "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog",
    "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella",
    "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball", "kite",
    "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle",
    "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
    "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant",
    "bed", "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone",
    "microwave", "oven", "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors",
    "teddy bear", "hair drier", "toothbrush",
]  # fmt: skip
ALIAS = {
    "celular": "cell phone",
    "telefone": "cell phone",
    "notebook": "laptop",
    "mouse": "mouse",
    "teclado": "keyboard",
    "controle": "remote",
    "livro": "book",
    "caneca": "cup",
    "copo": "cup",
    "garrafa": "bottle",
    "mochila": "backpack",
    "bolsa": "handbag",
    "guarda-chuva": "umbrella",
    "tesoura": "scissors",
    "relógio": "clock",
    "cadeira": "chair",
    "sofá": "couch",
    "vaso": "vase",
}
assert len(COCO_NOMES) == 80, "COCO tem 80 classes"


@dataclass(frozen=True)
class Deteccao:
    label: str
    confidence: float
    x: int  # centro, em pixels da imagem original
    y: int
    w: int
    h: int


def nome_em_ingles(objeto: str) -> str:
    """O nome que o modelo pediu, no rótulo do COCO (ou o próprio texto, se não houver alias)."""
    chave = " ".join(objeto.lower().split())
    return ALIAS.get(chave, chave)


def _nms(caixas: np.ndarray, confs: np.ndarray, classes: np.ndarray) -> list[int]:
    ordem = list(np.argsort(-confs))
    mantidas: list[int] = []
    while ordem:
        i = ordem.pop(0)
        mantidas.append(int(i))
        ordem = [
            j for j in ordem if classes[j] != classes[i] or _iou(caixas[i], caixas[j]) <= IOU_MAX
        ]
    return mantidas


def _iou(a: np.ndarray, b: np.ndarray) -> float:
    x1, y1 = max(a[0], b[0]), max(a[1], b[1])
    x2, y2 = min(a[2], b[2]), min(a[3], b[3])
    inter = max(0.0, x2 - x1) * max(0.0, y2 - y1)
    area = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return float(inter / area) if area > 0 else 0.0


def decodificar_saida(
    saida: np.ndarray, escala: float, pad_x: int, pad_y: int, *, limiar: float = LIMIAR
) -> list[Deteccao]:
    """Saída do YOLOv8 (1×84×N, ou 84×N) → detecções na imagem original."""
    p = np.squeeze(saida, 0) if saida.ndim == 3 else saida
    p = p.T  # (N, 4 + classes): cx, cy, w, h, scores...
    scores = p[:, 4:]
    classes = scores.argmax(axis=1)
    confs = scores.max(axis=1)
    manter = confs >= limiar
    p, classes, confs = p[manter], classes[manter], confs[manter]
    if p.size == 0:
        return []
    cx, cy, w, h = p[:, 0], p[:, 1], p[:, 2], p[:, 3]
    caixas = np.stack([cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2], axis=1)
    caixas[:, [0, 2]] = (caixas[:, [0, 2]] - pad_x) / escala
    caixas[:, [1, 3]] = (caixas[:, [1, 3]] - pad_y) / escala
    dets: list[Deteccao] = []
    for i in _nms(caixas, confs, classes):
        x1, y1, x2, y2 = caixas[i]
        nome = COCO_NOMES[int(classes[i])] if int(classes[i]) < len(COCO_NOMES) else "?"
        dets.append(
            Deteccao(
                label=nome,
                confidence=round(float(confs[i]), 3),
                x=int((x1 + x2) / 2),
                y=int((y1 + y2) / 2),
                w=int(x2 - x1),
                h=int(y2 - y1),
            )
        )
    return dets


class Detector:
    """YOLO nano em ONNX. A sessão é criada na primeira detecção (import tardio)."""

    def __init__(self, modelo: Path, *, sessao: Any = None) -> None:
        self.modelo = modelo
        self._sessao = sessao
        self._entrada: str | None = None

    def _sess(self) -> Any:
        if self._sessao is None:
            if not self.modelo.is_file():
                raise RuntimeError("modelo de detecção não encontrado (ORION_OBJECT_MODEL)")
            import onnxruntime  # type: ignore[import-not-found]

            self._sessao = onnxruntime.InferenceSession(
                str(self.modelo), providers=["CPUExecutionProvider"]
            )
        if self._entrada is None:
            self._entrada = self._sessao.get_inputs()[0].name
        return self._sessao

    def detectar(self, bgr: np.ndarray) -> list[Deteccao]:
        import cv2  # type: ignore[import-not-found]

        sess = self._sess()
        h0, w0 = bgr.shape[:2]
        escala = min(ENTRADA / w0, ENTRADA / h0)
        nw, nh = max(1, round(w0 * escala)), max(1, round(h0 * escala))
        tela = np.full((ENTRADA, ENTRADA, 3), 114, dtype=np.uint8)
        pad_x, pad_y = (ENTRADA - nw) // 2, (ENTRADA - nh) // 2
        tela[pad_y : pad_y + nh, pad_x : pad_x + nw] = cv2.resize(bgr, (nw, nh))
        x = tela[:, :, ::-1].transpose(2, 0, 1)[None].astype(np.float32) / 255.0
        saida = sess.run(None, {self._entrada: x})[0]
        return decodificar_saida(np.asarray(saida), escala, pad_x, pad_y)


class Avistamentos:
    """Rótulos vistos pela câmera (regra 64). Nunca guarda imagem nem a classe "person"."""

    def __init__(self, memory: Any, *, clock: Callable[[], float] = time.time) -> None:
        self.memory = memory
        self._clock = clock

    def registrar(self, deteccoes: list[Deteccao]) -> int:
        agora = self._clock()
        guardar = [d for d in deteccoes if d.label not in NUNCA_GUARDAR]
        with self.memory.transaction() as c:
            c.executemany(
                "INSERT INTO object_sightings(ts, label, confidence, x, y) VALUES (?,?,?,?,?)",
                [(agora, d.label, d.confidence, d.x, d.y) for d in guardar],
            )
            c.execute("DELETE FROM object_sightings WHERE ts < ?", (agora - RETENCAO_S,))
        return len(guardar)

    def onde_vi(self, objeto: str, limite: int = 5) -> list[dict[str, Any]]:
        rotulo = nome_em_ingles(objeto)
        if not rotulo or rotulo in NUNCA_GUARDAR:
            return []
        agora = self._clock()
        linhas = self.memory.query(
            "SELECT ts, confidence, x, y FROM object_sightings WHERE label=? AND ts>=?"
            " ORDER BY ts DESC LIMIT ?",
            (rotulo, agora - RETENCAO_S, max(1, min(limite, 20))),
        )
        return [
            {"ts": float(r["ts"]), "confianca": float(r["confidence"]), "x": r["x"], "y": r["y"]}
            for r in linhas
        ]


def olhar_uma_vez(
    *,
    foto: Callable[[], bytes],
    detector: Detector,
    avistamentos: Avistamentos,
    pode_olhar: Callable[[], bool],
) -> int:
    """Um quadro: detecta e guarda só os rótulos. `pode_olhar` é a presença (regra 66)."""
    from .camera import decodificar

    if not pode_olhar():
        return 0
    dets = detector.detectar(decodificar(foto()))
    return avistamentos.registrar(dets)
