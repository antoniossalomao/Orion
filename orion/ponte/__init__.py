"""Ponte de desktop (E2, regra 50): o processo que o navegador não consegue ser.

O Orion é servidor + navegador: não há como registrar tecla global, ficar na bandeja nem colar
texto no programa em foco. `orion ponte` sobe esse processo, à parte do servidor:

- `hub` / `rotas`: o lado do **servidor** (o WebSocket `/ws/ponte`, os comandos que ele manda e
  as poucas rotas que o token da ponte alcança);
- `config`: o mapa de teclas globais (`ORION_HOTKEYS`);
- `nucleo`: o lado do **cliente**, sem importar nada de sistema (teclado, bandeja, janela e área
  de transferência entram por construtor, como em `orion.wake`);
- `adaptadores`: as implementações de verdade (pynput, pystray, mss, tesseract), importadas só
  quando a ponte sobe.
"""
