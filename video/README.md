# Vídeo de lançamento (1920x1080)

`render(t)` determinístico em `index.html`, capturado frame a frame pelo Playwright e codificado com ffmpeg.

1. Telas: `cargo run --release -p orion-app --features snap --bin orion-snap -- <pasta>` e copie os PNG para `shots/` (sem o sufixo `-tiny-skia`).
2. Grafo: `python3 -I graph_build.py <caminho do vault>` gera `graph.json` (não versionado: tem os títulos das notas).
3. Narração: `ELEVEN_API_KEY=... python3 tts.py <voice_id>` (roteiro em `roteiro.json`), depois `python3 timeline.py`.
4. Trilha e mix: `python3 music.py s3 && python3 mix.py`.
5. `node prep.js && node render.js _play.html orion-lancamento-1080p.mp4`.

A conversa do Telegram na cena de aprovação é ilustrativa; a aprovação por botão existe no código.
