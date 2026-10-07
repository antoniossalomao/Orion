# ORION — Operação: ligar e usar no dia a dia

> O que configurar para o Orion novo rodar de verdade: login, acesso de fora (Tailscale), servidores
> MCP, ferramentas do computador e do celular. Criado em 06/10/2026. O que é **seu** (chaves, contas,
> instalar programas) está marcado **(você)**. Tudo aqui foi testado com serviços de mentira; o que só
> o uso real confirma está em [ORION_MELHORIAS.md](ORION_MELHORIAS.md) ("O que não foi verificado").

## 1. Subir

```
uv sync
uv run orion            # http://127.0.0.1:8000 (interface em /ui/)
```

Configuração por variáveis `ORION_*` ou por um `.env` na pasta onde o comando roda. Segredos
(chaves de API) podem ficar no cofre do sistema em vez do `.env` (`orion.secrets`: ambiente primeiro, cofre depois).

## 1.1 Executável do Windows (`orion.exe`)

O workflow **Orion.exe (Windows)** (`.github/workflows/build-exe.yml`) compila o Orion com PyInstaller,
**sobe o executável de verdade** e roda o teste de fumaça (`scripts/smoke_exe.py`: `/health`, interface
empacotada, login, troca de senha) antes de entregar o `Orion-windows-x64.zip`.

- Rodar: Actions › *Orion.exe (Windows)* › *Run workflow*, e baixar o artefato; ou criar uma tag `v*`
  (`git tag v0.1.0 && git push --tags`), que publica um *release* com o zip.
- Usar: descompactar, dar dois cliques em `orion.exe` (abre o navegador em `/ui/`), entrar com
  **`admin` / `261210@`** e **trocar a senha** em Configurações › Conexão. O `LEIA-ME.txt` do zip explica o resto.
- A senha de fábrica só existe no `.exe` (`ORION_SEED_DEFAULT_PASSWORD`, ligado pelo `orion_exe.py`). Ela está
  num repositório público: o app avisa depois do login e **recusa subir exposto na rede** (Tailscale) enquanto
  não for trocada. `orion.exe set-password` também troca.
- O executável não é assinado: o Windows mostra o aviso do SmartScreen. Os dados ficam em `%LOCALAPPDATA%\orion`.
- O ícone (`assets/orion.ico`) sai de `scripts/gerar_icone.py` (a constelação, minimalista).
- Verificado no Linux (PyInstaller + teste de fumaça); o Windows só roda no CI.

## 2. Login

```
uv run orion set-password        # pede a senha duas vezes (mínimo 12 caracteres)
```

- A senha vira um hash PBKDF2 em `<pasta de dados>/auth.db`. Esse arquivo **não** entra no backup que vai para
  a nuvem; restaurar o backup num computador novo exige rodar `set-password` de novo (de propósito).
- A tela de entrada pede **usuário** (`ORION_AUTH_USER`, padrão `admin`) e senha; errar um dos dois dá a mesma resposta. Quem entra ganha uma sessão (cookie `httpOnly`, `SameSite=Strict`) que vale 7 dias
  (`ORION_SESSION_TTL_H`). Trocar a senha encerra todas as sessões abertas.
- 5 senhas erradas travam o cliente por 1 minuto, e a trava dobra a cada rodada (até 1 h). Há também um teto
  global de 30 erros em 15 minutos.
- `ORION_ADMIN_TOKEN` (16+ caracteres) continua valendo como credencial de **máquina** (scripts, `curl`, o app
  desktop). Quem usa o navegador não precisa dele.
- Sem senha e sem token, a API fica desligada (503). `/health` sem login só diz `status` e `version`.

## 3. Acesso de fora (Tailscale)

Regra 17: um host que não seja local em `ORION_ALLOWED_HOSTS` só sobe **com login** (senha ou token). O Orion
continua escutando só em `127.0.0.1`; quem expõe na rede privada é o Tailscale.

1. **(você)** Instalar o Tailscale no notebook e no celular, com a mesma conta.
2. **(você)** Pôr o notebook na rede: `tailscale serve --bg 8000` (confira `tailscale serve --help`: a sintaxe muda
   entre versões). Isso dá um endereço HTTPS como `https://notebook.tail1234.ts.net` que só a sua rede alcança.
3. No `.env`:

   ```
   ORION_ALLOWED_HOSTS=["127.0.0.1","localhost","notebook.tail1234.ts.net"]
   ORION_COOKIE_SECURE=true
   ```

4. `uv run orion set-password` (se ainda não fez) e reiniciar.

Limites que valem saber:

- Atrás do `tailscale serve` o servidor enxerga todo cliente como `127.0.0.1`: o bloqueio por cliente do login
  vira um bloqueio global. É mais restritivo, não menos seguro.
- A interface web é **só para desktop** (decisão #5 do NUCLEO, fechada em 07/10/2026: não haverá PWA nem layout
  para celular). No celular o caminho é o Telegram; abrir `/ui/` no celular funciona mal e assim continuará.
- Nunca use `ORION_ALLOWED_HOSTS=["*"]` nem `ORION_ALLOW_PUBLIC_BIND=true` fora de teste.

## 4. Ferramentas

| Variável | Liga | Cuidado |
|---|---|---|
| `ORION_DESKTOP_TOOLS=true` | Agir no computador: comando, arquivos, documentos, área de transferência, notificação, abrir app, Git somente-leitura, saúde, processos em segundo plano, vigilância de pastas | Tudo passa pela política: leitura roda, escrita nas pastas seguras roda com log, o resto pede o seu aval no canal |
| `ORION_VISION_TOOLS=true` (além do desktop) | `capturar_tela`, `explicar_tela`, `analisar_imagem` (`ORION_VISION_MODEL` troca o modelo só para visão) | A tela mostra o que você faz: `explicar_tela` **sempre pede aprovação** (a imagem inteira vai ao provedor do modelo) e apaga a captura depois; `capturar_tela` só grava em `<dados>/capturas` (guarda as últimas 20). Precisa de um modelo que aceite imagem; no Linux, de `grim`, `scrot` ou `imagemagick` |
| (já vêm com o desktop) | `controlar_midia` (tocar/pausar, próxima, anterior, volume, mudo) e `controlar_janela` (listar, focar, minimizar, maximizar, restaurar, fechar) | Janela **sempre pede aprovação** (fechar pode perder trabalho não salvo) e busca ambígua não age. Mídia: Windows, macOS (Spotify ou Music) e Linux (`playerctl` + `wpctl`/`pactl`/`amixer`). Janela: Windows e Linux X11 (`wmctrl`); **não existe no macOS** |
| `ORION_WEB_TOOLS=true` | `buscar_url`, `consultar_clima`, `pesquisar_com_ia`, `pesquisar_internet` (precisa de `ORION_BRAVE_API_KEY`) e `gerar_imagem` (Gemini; chave de imagem, de busca ou de embeddings) | Página lida pode mandar o modelo buscar outra URL com dados na query (exfiltração por GET): depois de ler conteúdo externo, `buscar_url` e `navegar_web` **pedem o seu aval a cada uso**, com a URL inteira no cartão. Nunca foram chamadas contra as APIs reais |
| `ORION_SEARCH_API_KEY` | Chave do Gemini para `pesquisar_com_ia` (sem ela vale `ORION_EMBED_API_KEY`) | **(você)** conta no Google AI Studio |
| `ORION_WEATHER_CITY` | Cidade padrão do clima (padrão: Marília) | |
| `ORION_MCP_CONFIG` | Caminho do `mcp.json` (padrão: `<pasta de dados>/mcp.json`) | Ver §5 |
| `ORION_MCP_ENABLED=false` | Desliga o MCP sem apagar o arquivo | |

Pastas seguras (escrita sem pedir): Documents, Downloads e Desktop (`ORION_EXTRA_SAFE_ROOTS` para o Documents
do OneDrive, por exemplo). Escrever no código do Orion, em `.env`/`.ssh`/autostart ou fora dessas pastas pede aprovação.

### 4.1 Painel

A tela **Painel** (`#/painel`, `Alt+6`, comando `/painel` no campo de mensagem) e o `GET /painel` mostram num lugar só: cada
endpoint de modelo (funcionando, instável, **em quarentena por cota** e quando volta, último erro), o uso do dia das CLIs
(`claude`, `codex`, `gemini`), as aprovações esperando você, o que a política decidiu nas últimas 24 h, jobs, memória, Telegram e
servidores MCP. Atualiza sozinha a cada 10 s e destaca em texto o que pede atenção.

**Limite:** os números dos modelos são o que o Orion viu **desde que subiu** (reiniciar zera); não são a cota do provedor,
que só o OmniRoute conhece. Com o OmniRoute, o painel mostra também **quem serviu** as respostas ("Serviu: gemini ×30 ·
groq ×8"), lido dos cabeçalhos `X-OmniRoute-Provider` e `-Decision` (formato da documentação, não testado num OmniRoute
real). A cota de verdade está nos endpoints de gerenciamento do OmniRoute (`/api/rate-limits`, `/dashboard/free-tiers`),
com credencial própria que o Orion não usa. O painel não mostra os argumentos das ações nem nenhum segredo.

### 4.2 Roteamento por tipo de tarefa

Com `ORION_GATEWAY_MODEL_FAST` e/ou `ORION_GATEWAY_MODEL_HEAVY` (e `ORION_VISION_MODEL` para foto), o Orion escolhe o
modelo pela mensagem: conversa curta e comando de ferramenta vão ao rápido; código, análise, texto longo e pedidos
de várias etapas vão ao pesado; mensagem com foto vai ao de visão. Sem nenhum desses, tudo segue em
`ORION_GATEWAY_MODEL`, que também é o **reserva** de cada camada (se o rápido ou o pesado falhar, cai nele).

- A decisão é uma pontuação, não um modelo: tamanho, código ou erro colado, verbos como *analise*, *compare*, *refatore*,
  várias perguntas, tema técnico. O motivo fica registrado em cada resposta (proveniência) e o painel conta as camadas.
- Errou? Comece a mensagem com `#pesado`, `#rapido` ou `#visao` e a camada é forçada.
- No OmniRoute o "modelo" pode ser um combo seu (ex.: um combo `rapido` só com free tiers rápidos).
- É uma heurística: vai errar alguns casos. Ajustar os pesos é editar `orion/router.py`.

### 4.3 Voz (fase 6)

Duas coisas diferentes, cada uma com a sua chave e o seu risco. As duas nascem **desligadas**.

**A — falar com o Orion (microfone do front).** Clique no microfone do campo de mensagem (ou `Ctrl+K › Falar com o Orion`),
fale e clique de novo para enviar; `Esc` descarta. O que ele entendeu aparece como balão seu, a resposta vem em texto e
falada. É um turno comum do agente: memória, ferramentas e política valem como se você tivesse digitado. **Aprovar uma
ação não se faz por voz**: a resposta avisa e o cartão aparece no chat.

```
ORION_VOICE_ENABLED=true
ORION_TRANSCRIBE_API_KEY=...        # Whisper no Groq (a mesma do Telegram)
# ORION_VOICE_SPEAK=false           # só texto: nada da resposta vai para a Microsoft
```

Para onde vai o quê: a sua **fala gravada** vai ao Groq; o **texto da resposta** (já sem markdown, código e endereços,
até ~700 caracteres) vai ao serviço de voz da Microsoft (edge-tts, voz `pt-BR-AntonioNeural`). Sem `ORION_VOICE_SPEAK`
o segundo passo some.

**B — voz ao vivo (botão de ondas).** Conversa em tempo real com o Gemini Live, voz Charon. O **áudio do microfone vai ao
Google** e o modelo **só conversa**: sem ferramentas, sem memória, sem acesso ao computador (se você pedir uma ação, ele
manda usar o chat). A conversa não entra na memória; início e fim de cada sessão ficam no audit (`voz_ao_vivo`).

```
ORION_VOICE_LIVE_ENABLED=true
ORION_VOICE_LIVE_API_KEY=...        # vazio: usa a de busca/embeddings (mesmo Google AI Studio)
# ORION_VOICE_LIVE_MAX_MIN=20       # teto de uma sessão
```

- **Parar a resposta:** o mesmo botão do microfone vira "Parar a resposta" enquanto o Orion pensa ou fala (`Esc` e o botão de parar do chat também).
- **Ver se está pronta:** o Painel (`#/painel`) mostra a voz por clique e a ao vivo (turnos, sessões, minutos, falhas) e o `/health` traz `components.voice`.
- O navegador só libera o microfone em `https://` ou `localhost`: pelo Tailscale, use `tailscale serve` (HTTPS) e não o IP.
- Sem login, sem `Origin` igual ao `Host` ou com a voz desligada, o WebSocket responde com o motivo e fecha.
- **Palavra de ativação ("Orion"):** existe desde 07/10, em §4.4. Só abre o microfone; confirmar ação continua sendo o botão (regra 2).
- Nada disto foi chamado contra Groq, edge-tts ou Gemini Live de verdade ([ORION_MELHORIAS.md](ORION_MELHORIAS.md), sexta rodada).

### 4.4 Palavra de ativação ("Orion", escuta contínua)

Um agente fica **ouvindo o microfone o tempo todo** neste computador. Ele só **detecta** a palavra: enquanto você não diz
"Orion", o áudio existe apenas no quadro de 80 ms que está sendo analisado (nada vai para o disco, para a rede nem para a
memória). Quando ouve: bipe, grava o que você diz até o silêncio e manda pela mesma via do botão de microfone (Groq →
agente → edge-tts), tocando a resposta no alto-falante. **Aprovar nunca é por voz** (regra 2): o Orion avisa e o cartão
aparece na tela. Regra 38 em [ORION_REGRAS.md](ORION_REGRAS.md).

**Escolha o detector** (nenhum precisa gravar amostras suas):

| | Vosk (começa por aqui) | openWakeWord |
|---|---|---|
| O que é | reconhecimento offline restrito a um vocabulário pequeno | modelo `.onnx` treinado só em "orion" |
| Para ligar | baixar `vosk-model-small-pt-0.3` (31 MB, [alphacephei.com/vosk/models](https://alphacephei.com/vosk/models)) e descompactar | gerar o modelo no Colab oficial ("automatic_model_training", voz sintética) e salvar o `.onnx` |
| Medido (voz sintética) | 18/20 acertos, 0/60 falsos alarmes; 11/12 e 1/30 num lote feito para confundir; ~7% de um núcleo | **não medido**: não consegui rodá-lo (meu ambiente de teste é Linux, onde a dependência `tflite-runtime` não instala); no Windows e no macOS deve instalar, mas ninguém testou |
| Instalar | `uv sync --extra wake-vosk` | `uv sync --extra wake` |

**Passo a passo (Vosk):**
1. `uv sync --extra wake-vosk` (o PortAudio já vem no pacote do `sounddevice` no Windows e no macOS).
2. Baixe e descompacte o modelo; anote a pasta.
3. `uv run orion wake-test --listar` mostra os microfones; no `.env`, `ORION_WAKE_DEVICE=<número ou parte do nome>` se não for o padrão.
4. Teste sem ligar nada do Orion:
   `ORION_WAKE_ENGINE=vosk ORION_WAKE_MODEL=<pasta> uv run orion wake-test` (mostra o nível do microfone e diz "✔ palavra ouvida").
5. **Meça com a sua voz:** grave 20 frases com "Orion" e 20 sem (TV, música, conversa), salve como `pos_01.wav`, `neg_01.wav`... (WAV 16 kHz mono) e rode
   `uv run python scripts/wake_eval.py vosk <pasta-do-modelo> <pasta-dos-wavs>`. Falso alarme custa mais que ativação perdida.
6. Ligue de verdade no `.env`:
   ```
   ORION_VOICE_ENABLED=true            # a escuta usa a via da voz (Groq + edge-tts)
   ORION_TRANSCRIBE_API_KEY=...        # a chave do Groq
   ORION_WAKE_ENABLED=true
   ORION_WAKE_ENGINE=vosk
   ORION_WAKE_MODEL=C:\caminho\vosk-model-small-pt-0.3
   ```
   Com openWakeWord: `ORION_WAKE_ENGINE=openwakeword`, `ORION_WAKE_MODEL=<arquivo>.onnx` e `ORION_WAKE_THRESHOLD` (sobe se ativar sem você falar).
7. No Painel (`#/painel`) a linha **Palavra de ativação** mostra "ouvindo", quantas ativações houve e, se não subiu, o motivo.
   O botão **Pausar escuta** (ou `POST /voz/escuta {"ativa": false}`) para o detector sem fechar o Orion.

**Se ativar demais:** suba `ORION_WAKE_THRESHOLD` (openWakeWord) ou diminua `ORION_WAKE_MAX_PER_HOUR`; o teto por hora ignora o excesso
(e registra "recusada" no audit). **Se não ativar:** confira o microfone com `wake-test`, aproxime-se, e tente o limiar menor.

**O que sai do computador:** só **depois** da palavra, a fala seguinte (Groq) e o texto da resposta (Microsoft, `ORION_VOICE_SPEAK=false`
corta). Tocar a resposta usa o PowerShell no Windows e `afplay` no macOS. Para o notebook ficar
sempre escutando, junte com `orion autostart`. **Não validado com microfone, sala e voz reais** ([ORION_MELHORIAS.md](ORION_MELHORIAS.md), sétima rodada).

## 5. Servidores MCP

E-mail, agenda, navegador e busca não são escritos aqui: entram por servidores MCP prontos.

1. Copie `mcp.example.json` para `<pasta de dados>/mcp.json` e deixe só os servidores que quiser (`"enabled": true`).
2. Classifique **você** cada ferramenta que quiser sem pergunta (`"risk": "read"` ou `"write"`). O que não estiver em
   `tools` usa `default_risk` (`exec` = pede aprovação sempre). A classe nunca vem do servidor.
3. Marque `"external": true` nos servidores que trazem conteúdo da rua (web, e-mail, navegador): depois de lerem algo,
   a sessão passa a pedir aprovação para escrever e executar.
4. Segredos do servidor: `"env": {"CHAVE": "${NOME_NO_COFRE}"}` e guarde o valor no ambiente ou no cofre do sistema.
5. Conferir: `uv run orion mcp-check` sobe os servidores e lista cada ferramenta com a classe que a política usará.

**(você)** Os pacotes do exemplo precisam de `npx` (Node) ou `uvx`. **Fixe a versão** de cada pacote (`pacote@1.2.3`): `npx -y` sem versão baixa o que for mais novo, e pacote npm/PyPI comprometido é risco de cadeia de suprimentos. Dê a cada servidor só a pasta de que precisa (nada de `C:\` nem o perfil inteiro). Para e-mail e agenda do Google, escolha um servidor
MCP em que você confie e nunca classifique envio de e-mail como `read` nem `write`.

### 5.1 E-mail e agenda do Google (servidor `google` do `mcp.example.json`)

O exemplo usa o **`workspace-mcp`** (PyPI, versão fixada). É o único que foi **conferido de verdade** (subi o
servidor e listei as ferramentas): com `--permissions gmail:drafts calendar:full` ele **não registra ferramenta
de envio de e-mail**, só leitura, rascunho e agenda. As ferramentas que o exemplo libera:

| Ferramenta | Classe no exemplo | Observação |
|---|---|---|
| `search_gmail_messages`, `get_gmail_message_content`, `get_gmail_messages_content_batch`, `get_gmail_thread_content`, `get_gmail_threads_content_batch`, `list_gmail_labels` | read (externo) | O texto do e-mail é de terceiros: depois de ler, a sessão passa a pedir aprovação para escrever e executar |
| `draft_gmail_message` | write | Só cria rascunho; quem envia é você, no Gmail |
| `list_calendars`, `get_events`, `query_freebusy` | read (externo) | Título e descrição de evento também são de terceiros |
| `manage_event` | exec (confirma sempre) | Cria, altera **e apaga** evento |
| `start_google_auth` | exec | Só na primeira vez |

Ficam de fora (por `allow`): anexos, rótulos que alteram mensagens, ausência, foco, criar agenda.

**(você)** Passo a passo:

1. No Google Cloud Console, crie um projeto, ative as APIs Gmail e Calendar e crie um **cliente OAuth** do tipo
   "Aplicativo para computador". Anote o ID e o segredo.
2. Guarde no ambiente ou no cofre: `ORION_GOOGLE_CLIENT_ID` e `ORION_GOOGLE_CLIENT_SECRET`.
3. Copie o bloco `google` para o seu `mcp.json`, tire o `"enabled": false` e rode `uv run orion mcp-check`.
4. Na primeira conversa, peça "olha minha agenda de hoje": o servidor conduz a autorização (`start_google_auth`, que
   pede o seu aval). **Não testei esse fluxo** (exige a sua conta e o seu cliente OAuth): confira onde ele guarda o
   token (veja a documentação do `workspace-mcp`), trate essa pasta como segredo e deixe-a fora de qualquer backup
   que vá para a nuvem.

**Outro servidor?** Vale se cumprir: (a) versão **fixada**; (b) dá para **esconder** o envio, seja por opção do
servidor, seja por `allow` (o Orion só mostra ao modelo o que está em `allow`); (c) a lista que o `mcp-check` mostra
não tem nada de envio, exclusão em massa ou "executar script" classificado como `read`/`write`. Servidores
populares expõem `send_email`: com `allow` ele some do modelo, mas prefira os que nem o registram. Os outros eu **não** testei.

**Agenda no briefing matinal (regra 37).** Com o servidor `google` funcionando e o login feito (passo 4), o briefing pode
trazer a agenda do dia. Não passa pelo modelo: o job chama a ferramenta `get_events` direto, com o seu e-mail e o dia de hoje.
```
ORION_BRIEFING_AT=07:30
ORION_BRIEFING_CALENDAR_EMAIL=voce@gmail.com
# ORION_BRIEFING_CALENDAR_TOOL=google__get_events   # padrão; precisa ser "read" no mcp.json
```
Só liga se o `mcp.json` classifica a ferramenta como `read` (no exemplo, `get_events` já é). A resposta é limpa (sem links nem IDs),
limitada a 10 linhas e vai para o aviso diário e para o `/briefing` do Telegram; cada consulta fica no audit como `briefing_agenda`.
Se o servidor cair ou o login expirar, o briefing sai com "agenda indisponível (motivo)". **Ainda não vi o texto que o `get_events` devolve
com uma conta real**: se as linhas vierem estranhas na primeira vez, me mande um exemplo que eu ajusto a limpeza.

## 6. Celular (Telegram)

| Variável | Para quê |
|---|---|
| `ORION_TELEGRAM_TOKEN`, `ORION_TELEGRAM_ALLOWED_USERS` | O bot e quem pode falar com ele (**(você)** criar no @BotFather; o seu ID numérico vem do @userinfobot). Sem a lista o bot não sobe |
| `ORION_TRANSCRIBE_API_KEY` | Voz: mensagem de voz vira texto (Whisper no Groq, grátis). O texto entendido aparece antes da resposta. Sem a chave o bot pede para escrever. Com a chave (e `ORION_DESKTOP_TOOLS`) também existe a ferramenta `transcrever_audio` para arquivos de áudio |
| `ORION_TRANSCRIBE_URL`, `ORION_TRANSCRIBE_MODEL` | Outro provedor compatível com a API de transcrição da OpenAI |

| Comando | O que faz |
|---|---|
| `/capturar <texto ou link>` | Guarda na hora como nota no vault, sem passar pelo modelo |
| `/capturar` e depois **texto, link, foto ou voz** | A próxima mensagem (até 5 minutos; qualquer outro comando cancela) vira nota. A voz é transcrita (precisa de `ORION_TRANSCRIBE_API_KEY`) e o texto entendido aparece antes de guardar |
| `/briefing` | O resumo do dia, na hora |
| `/painel` | Modelos (e quarentena por cota), CLIs, aprovações, política das últimas 24 h, jobs e MCP |

Captura: precisa de `ORION_VAULT_DIR`. A nota vai para `<vault>/<ORION_CAPTURE_FOLDER>` (padrão `00 Inbox`), com o
frontmatter mínimo (`date`, `hora`, `fonte: telegram`, `tags: [captura, ...]`) e **sem `type`**: o que é captura ainda
não foi triado. Foto vai para `anexos/` ao lado, e a nota a incorpora. O link é guardado como veio (não busco a página).
Briefing: `ORION_BRIEFING_AT=07:30` envia um aviso por dia com lembretes atrasados e de hoje, agendamentos de hoje e
tarefas em aberto (se o Orion só subir depois da hora, ainda sai em até 6 h).

Foto: vai ao modelo como imagem só naquele turno (o histórico guarda um aviso). Só funciona se o modelo do gateway
aceitar imagem. Aprovar ação continua **só por botão**, nunca por frase ou por voz.

## 7. Checklist antes de abrir para fora

- [ ] `uv run orion set-password` feito e a senha é longa e única.
- [ ] `ORION_ALLOWED_HOSTS` só com o nome do Tailscale (nada de `*`).
- [ ] `ORION_COOKIE_SECURE=true` se for por HTTPS.
- [ ] Servidores MCP com `default_risk` `exec` e só as leituras que você conhece classificadas.
- [ ] `ORION_WEB_TOOLS` ligado só se precisar.
- [ ] Backup diário apontando para uma pasta sincronizada (`ORION_BACKUP_DIR`) e um `orion restore` já testado.
- [ ] Bot do Telegram do legado parado (o Telegram recusa dois clientes no mesmo token).
