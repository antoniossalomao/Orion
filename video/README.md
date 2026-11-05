# Vídeo de lançamento (1920x1080)

`render(t)` determinístico em `index.html`, capturado frame a frame pelo Playwright e codificado com ffmpeg.

1. Telas: `cargo run --release -p orion-app --features snap --bin orion-snap -- <pasta>` e copie os PNG para `shots/` (sem o sufixo `-tiny-skia`).
2. Grafo: `python3 -I graph_build.py <caminho do vault>` gera `graph.json` (não versionado: tem os títulos das notas).
3. Narração: `ELEVEN_API_KEY=... python3 tts.py <voice_id>` (roteiro em `roteiro.json`), depois `python3 timeline.py`.
4. `node prep.js && node sfx_export.js` (efeitos declarados em `sfxList()` da cena).
5. Trilha e mix: `python3 music_ad.py s3 s12 s13 && python3 mix.py`.
6. `node render.js _play.html orion-lancamento-1080p.mp4`.

Logo vetorial: `logo/logo.html`; `node logo/export.js` gera `orion-logo.png` (3780x1000, transparente) e `orion-simbolo.png` (1024²).

A conversa do Telegram na cena de aprovação é ilustrativa; a aprovação por botão existe no código.
