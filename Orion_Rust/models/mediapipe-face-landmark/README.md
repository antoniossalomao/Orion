# MediaPipe Face Landmark (ONNX)

- Arquivo: `face_landmark_detector.onnx` (2,45 MB, arquivo único)
- SHA-256: `c745c5395be8c6abad372f82a7de30b6b0e340019df4526f2930ce23d432716c`
- Entrada: `image` float32 1×3×192×192, valores em 0..1 (RGB)
- Saídas: `scores` (1,) e `landmarks` (1×468×3, x/y em pixels de 192 e z)
- Origem: export ONNX do MediaPipe Face Landmark, do repositório
  `Heliosoph/mediapipe-face-onnx` (float, ONNX 1.24.3, gerado com QAIRT 2.42).
  Os dados externos (`.data`) foram embutidos no próprio `.onnx`.
- Licença: Apache-2.0 (licença do repositório de origem e do MediaPipe).
  Atribuição: Google LLC (MediaPipe) e Heliosoph (export ONNX).
