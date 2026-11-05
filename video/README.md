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

## Vídeo do README (`readme/`)

Sem narração, com elementos desenhados (HTML/SVG/canvas) e poucos prints. O ritmo vem de `SLOW` em `index.html`.

1. `cd readme && node prep.js` (injeta `../graph.json`) e `node ../sfx_export.js` (efeitos de `sfxList()`).
2. `node render_silent.js _play.html orion-apresentacao.mp4`.
3. `python3 music_slow.py <duração>`: trilha de 84 BPM + efeitos (usa `../sfx_lib.py`), gera `audio_readme.wav`; junte com `ffmpeg -i orion-apresentacao.mp4 -i audio_readme.wav -c:v copy -c:a aac -shortest`.
4. `python3 make_webp.py orion-apresentacao.mp4 orion-apresentacao.webp 70`: WebP animado (1280x720, 15 fps) que toca sozinho no README.
