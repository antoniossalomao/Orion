#!/usr/bin/env bash
# Gera as fotos do app (iced) e do protótipo e as junta lado a lado em <saida>/comparacao.
# Uso: tools/gerar.sh <orion-ui-mockup.html> <saida> [filtro]
set -euo pipefail
cd "$(dirname "$0")/.."
HTML="$1"; OUT="$2"; FILTRO="${3:-}"
mkdir -p "$OUT/iced" "$OUT/prototipo"
ICED_TEST_BACKEND=tiny-skia cargo run -q -p orion-app --features snap --bin orion-snap -- "$OUT/iced-bruto" $FILTRO >/dev/null
python3 - "$OUT" <<'PY'
import sys, glob, os
from PIL import Image
out = sys.argv[1]
for f in glob.glob(os.path.join(out, "iced-bruto", "*-tiny-skia.png")):
    im = Image.open(f).convert("RGB")
    im = im.resize((im.width // 2, im.height // 2), Image.LANCZOS)
    im.save(os.path.join(out, "iced", os.path.basename(f).replace("-tiny-skia", "")))
PY
node tools/foto-prototipo.js "$HTML" "$OUT/prototipo" $FILTRO
python3 tools/comparar.py "$OUT/prototipo" "$OUT/iced" "$OUT/comparacao" $FILTRO
