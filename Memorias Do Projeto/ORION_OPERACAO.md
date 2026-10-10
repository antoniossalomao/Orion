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

**Limite:** os números dos modelos são o que o Orion viu **desde que subiu** (reiniciar zera); não são a cota que o
provedor vê (outro programa com a mesma chave também gasta). Cada modelo aparece com o estado: funcionando, **em
quarentena** (429, volta pelo `Retry-After`), **em pausa por falhas** (3 falhas seguidas; a pausa dobra até 15 min) e
quantas vezes foi para o fim da fila por ter gasto o `limite_dia`. O painel não mostra os argumentos das ações nem
nenhum segredo.
Desde a E1 o painel também tem **Controle** (pânico e não perturbe), **Cota de hoje** e **Provedores** (a semana,
pelo registro de saída, que sobrevive a reinício): ver §17.

### 4.2 Provedores de modelo e roteamento por tipo de tarefa

O Orion fala **direto** com cada provedor (sem gateway externo; o OmniRoute foi descartado em 09/10/2026). Catálogo em
`orion/provedores.py`: `gemini`, `groq`, `cerebras`, `openrouter`, `mistral`, `github`, `nvidia`, `zai` (todos com
API compatível com a da OpenAI); outro provedor entra com `"url"` (https).

1. **(você)** crie a chave no site de cada provedor e guarde: `uv run orion chave groq` (cofre do SO; ou
   `ORION_KEY_GROQ` no ambiente).
2. Liste os provedores em `ORION_PROVEDORES` (JSON; **a ordem é a prioridade**). Por provedor: `padrao`, `rapido`,
   `pesado`, `visao` (o modelo de cada camada) e `limite_dia` (chamadas/dia que o Orion se permite):

   ```
   ORION_PROVEDORES=[{"id":"groq","rapido":"llama-3.3-70b-versatile","limite_dia":1000},{"id":"gemini","padrao":"gemini-2.5-flash","visao":"gemini-2.5-flash"}]
   ```

3. `uv run orion doctor` mostra quem ficou ligado e quem está sem chave.

Os modelos e limites gratuitos mudam sem aviso: por isso ficam na sua configuração, não no código. Cada modelo é um
endpoint: 429 põe **só aquele modelo** em quarentena; 3 falhas seguidas o pausam (60 s, dobrando até 15 min); com o
`limite_dia` gasto ele vai para o fim da fila (o chat nunca é bloqueado: se ninguém mais responder, ele ainda é
tentado). `ORION_GATEWAY_URL`/`_MODEL` continua valendo como um endpoint avulso, depois dos provedores.

Roteamento: com modelos de camada (`rapido`, `pesado`, `visao` em `ORION_PROVEDORES`, ou `ORION_GATEWAY_MODEL_FAST` e/ou `ORION_GATEWAY_MODEL_HEAVY` (e `ORION_VISION_MODEL` para foto), o Orion escolhe o
modelo pela mensagem: conversa curta e comando de ferramenta vão ao rápido; código, análise, texto longo e pedidos
de várias etapas vão ao pesado; mensagem com foto vai ao de visão. Sem nenhum desses, tudo segue em
`ORION_GATEWAY_MODEL`, que também é o **reserva** de cada camada (se o rápido ou o pesado falhar, cai nele).

- A decisão é uma pontuação, não um modelo: tamanho, código ou erro colado, verbos como *analise*, *compare*, *refatore*,
  várias perguntas, tema técnico. O motivo fica registrado em cada resposta (proveniência) e o painel conta as camadas.
- Errou? Comece a mensagem com `#pesado`, `#rapido` ou `#visao` e a camada é forçada.
- Numa camada, a ordem é: os modelos daquela camada (na ordem dos provedores), depois os `padrao` de cada um.
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

## 8. Memória da tela

**O que faz.** De tempos em tempos lê o texto que está na tela por OCR **local** e guarda só o texto, com
retenção curta. O modelo consulta por `buscar_tela` ("o que eu estava lendo ontem sobre Pix?").

**Como ligar.**
```
ORION_SCREEN_MEMORY=true
# ORION_SCREEN_INTERVAL_S=300            # uma captura a cada 5 min (mínimo 60)
# ORION_SCREEN_RETENTION_DAYS=7          # apaga o texto mais velho que isso (1 a 90)
# ORION_SCREEN_EXCLUDE=senha;banco;pix   # título de janela que casa com um item: nem captura
# ORION_SCREEN_ALLOW_UNKNOWN_TITLE=false # sem título da janela, não captura
# ORION_SCREEN_OCR_LANGS=por+eng
```

**Dependências.** **(você)** `tesseract` com o pacote de português (`por`): no Windows, o instalador do
UB Mannheim marcando "Portuguese"; no macOS, `brew install tesseract tesseract-lang`; no Linux,
`tesseract-ocr tesseract-ocr-por`. No Linux X11 também `xdotool` (título da janela) e `grim`, `scrot` ou
`imagemagick` (captura). `orion doctor` avisa se faltar o `tesseract`.

**O que sai do computador.** Nada. A imagem vai para um arquivo temporário apagado na hora; o OCR roda
aqui; linhas com cara de segredo, CPF ou cartão são descartadas antes de gravar. O texto só sai se você
perguntar algo no chat e o modelo chamar `buscar_tela` (aí os trechos achados vão ao provedor do modelo,
como qualquer resultado de ferramenta).

**Onde ver.** `orion tela` (estado e contagem), `/tela` no Telegram, Painel › Sistema e `GET /tela`.

**Pausar/desligar.** Pausar: `POST /tela/pausa {"ativa": false}` (retomar com `true`), `/tela pausar` no
Telegram. Apagar tudo o que foi guardado: `orion tela --limpar`, `DELETE /tela` ou `/tela limpar`.
Desligar: `ORION_SCREEN_MEMORY=false` e reiniciar.

**Regra.** 44 ([ORION_REGRAS.md](ORION_REGRAS.md)); código em `orion/screen_memory.py`.

## 9. Ciclo de sono

**O que faz.** Uma vez por noite revisa a memória: avisa fatos quase repetidos (nunca apaga), tira
relações entre coisas ("Antônio —estuda_em→ Unimar") e destila até 3 padrões a partir dos fatos novos.

**Como ligar.**
```
ORION_SLEEP_AT=03:00    # HH:MM; vazio = desligado
```
Precisa dos jobs ligados (`ORION_JOBS_ENABLED=true`, o padrão). Sem gateway de modelos só o passo dos
duplicados roda.

**Dependências.** Nenhuma além do gateway.

**O que sai do computador.** Uma chamada por noite ao gateway, só com **fatos já guardados** (nunca a
conversa crua nem conteúdo externo). A resposta é validada (tamanho, formato, nada com cara de segredo).

**Onde ver.**
- Duplicados: aviso na caixa de atividade (Painel › Atividade) e `orion fatos --duplicados`.
- Relações: arestas do grafo com `kind=sono`.
- Padrões: fatos com fonte `sono:destilado:<data>` na tela Conhecimento e em `orion fatos`.

**Pausar/desligar.** Apagar um padrão ou duplicado: `orion esquecer <id>` (ou pela tela Conhecimento).
Desligar: esvaziar `ORION_SLEEP_AT` e reiniciar.

**Regra.** 42; código em `orion/memory/sleep.py`.

## 10. Pesquisa noturna

**O que faz.** Na hora marcada pesquisa na internet os assuntos que **você** escolheu, um turno só
leitura por assunto, e grava um relatório no `00 Inbox` do vault.

**Como ligar.** As três juntas, senão não sobe (o log e o `orion doctor` dizem o que falta):
```
ORION_RESEARCH_AT=04:00
ORION_WEB_TOOLS=true
ORION_VAULT_DIR=C:\Users\voce\Obsidian\Vault
# ORION_RESEARCH_TOPICS=MCP segurança e novidades; Pix e Open Finance; LLMs locais   # até 5, separados por ;
```

**Dependências.** Gateway de modelos; chave do Gemini (`ORION_SEARCH_API_KEY` ou a de embeddings) para
`pesquisar_com_ia`, ou `ORION_BRAVE_API_KEY` para `pesquisar_internet`.

**O que sai do computador.** Os assuntos (como texto de busca) vão aos provedores de busca e ao gateway;
as páginas lidas voltam como conteúdo externo. Nada do seu computador além disso.

**Onde ver.** Uma nota por dia no `00 Inbox` (`<data hora> - Pesquisa noturna...`, tag `pesquisa`), com o
aviso "conteúdo de fontes externas". Cada assunto vira uma conversa no canal `pesquisa`.

**Pausar/desligar.** Esvaziar `ORION_RESEARCH_AT` e reiniciar.

**Regra.** 39; código em `orion/research.py`.

## 11. Leitura semanal

**O que faz.** Na segunda-feira, junto com o briefing, o modelo escreve até 5 linhas sobre o que merece
atenção na semana, a partir do resumo da semana que passou.

**Como ligar.**
```
ORION_WEEKLY_AI=true
ORION_BRIEFING_AT=07:30   # a leitura sai junto com o briefing de segunda: sem briefing, não sai
```

**Dependências.** Gateway de modelos.

**O que sai do computador.** **Uma** chamada por semana ao gateway, com o resumo semanal (contagens,
títulos de tarefas concluídas e fatos já guardados).

**Onde ver.** Aviso `semanal` ("💬 Leitura do Orion: ...") na caixa de atividade e no Telegram.

**Pausar/desligar.** `ORION_WEEKLY_AI=false` e reiniciar (o resumo semanal sem modelo continua saindo).

**Regra.** Segue a 37 (o resumo é determinístico) com uma chamada de modelo por cima; código em
`orion/briefing.py` (`build_weekly`) e `orion/jobs.py` (`_leitura_semanal`).

## 12. n8n

**O que faz.** Deixa o modelo disparar um workflow seu do n8n por webhook (`acionar_n8n`), escolhendo
**só o nome** entre os que você cadastrou.

**Como ligar.** JSON com nome e URL de cada webhook:
```
ORION_N8N_WEBHOOKS={"diario": "http://127.0.0.1:5678/webhook/diario", "planilha": "https://n8n.exemplo.com/webhook/abc"}
```
Exemplo de workflow: nó **Webhook** (método POST, caminho `diario`) → nó que usa `{{$json.body}}` (o
corpo JSON que o modelo mandou, até 8 mil caracteres) → nó **Respond to Webhook** com um texto curto.

**Dependências.** Um n8n rodando (local ou seu servidor).

**O que sai do computador.** O corpo JSON vai ao endereço do webhook (se o n8n é local, não sai daqui).

**Onde ver.** O cartão de aprovação mostra nome e corpo; a resposta do n8n aparece no chat (até 2 mil
caracteres, marcada como conteúdo externo).

**Pausar/desligar.** Esvaziar `ORION_N8N_WEBHOOKS` e reiniciar.

**Por que sempre confirma.** Um workflow pode fazer qualquer coisa no mundo (mandar e-mail, pagar,
apagar). Por isso `acionar_n8n` é **execução**: cada uso pede o seu aval no canal, e a resposta contamina
a sessão (pode trazer texto de terceiros).

**Regra.** 41; código em `orion/tools/n8n.py`.

## 13. Plugins e skills

**O que faz.** Skill é um texto de orientação (formato Agent Skills) que o modelo carrega quando precisa;
plugin é um pacote com skills e servidores MCP, instalado e concedido por você.

**Como ligar.** Já vêm ligados (`ORION_SKILLS_ENABLED=true`, `ORION_PLUGINS_ENABLED=true`); pastas em
`ORION_SKILLS_DIR` (padrão `<dados>/skills`) e `ORION_PLUGINS_DIR` (padrão `<dados>/plugins`).

Estrutura de uma skill (`<skills>/<nome>/SKILL.md`):
```
---
name: resumo-de-reuniao            # a-z, 0-9 e hífen; igual ao nome da pasta
description: Como resumir uma reunião em tópicos e próximos passos.
---
(corpo em Markdown, até 20 mil caracteres)
```
Estrutura de um plugin (`<pasta>/plugin.json`, até 200 arquivos e 5 MB, sem link simbólico):
```json
{"name": "pesquisa", "version": "1.0.0", "description": "Skills e busca para pesquisa",
 "mcp": {"busca": {"command": "uvx", "args": ["pacote@1.2.3"], "default_risk": "exec",
                   "tools": {"buscar": {"risk": "read"}}}}}
```
Skills do plugin ficam em `skills/` dentro da pasta.

```
uv run orion skills                         # lista as válidas e as rejeitadas, com o motivo
uv run orion plugin instalar <pasta>        # só valida e copia
uv run orion plugin conceder <nome>         # guarda o hash do pacote; vale depois de reiniciar
uv run orion plugin revogar <nome>
uv run orion plugin listar | atualizar <pasta> | remover <nome>
```

**Dependências.** As de cada servidor MCP do plugin (`npx`, `uvx`...).

**O que sai do computador.** A skill é texto: só nome e descrição entram no prompt (vão ao gateway como o
resto do contexto). Servidores MCP de plugin seguem a §5.

**Onde ver.** `orion skills`, `orion plugin listar`, a tela Conhecimento (cartão Plugins) e `GET /plugins`.

**Pausar/desligar.** `orion plugin revogar <nome>` (vale depois de reiniciar); qualquer mudança nos
arquivos do plugin também cancela a concessão. `ORION_SKILLS_ENABLED=false` ou `ORION_PLUGINS_ENABLED=false`
desligam tudo.

**Regra.** 40 (skill é texto) e 45 (plugin sem poder próprio); código em `orion/skills.py` e `orion/plugins.py`.

## 14. `orion transcrever`

**O que faz.** Transcreve um arquivo de áudio/vídeo ou um link para uma nota no `00 Inbox` do vault.

**Como ligar.** É um comando seu (nunca ferramenta do modelo):
```
uv run orion transcrever reuniao.mp4 --titulo "Reunião com o cliente"
uv run orion transcrever https://www.youtube.com/watch?v=...   # baixa só o áudio
```
Precisa de `ORION_TRANSCRIBE_API_KEY` (Groq) e `ORION_VAULT_DIR`.

**Dependências.** **(você)** `ffmpeg` no PATH; para link, `yt-dlp`.

**O que sai do computador.** **O áudio** vai ao provedor de transcrição (`ORION_TRANSCRIBE_URL`, Groq por
padrão), em pedaços de 20 min. O comando avisa e pergunta antes de enviar (`--sim` pula a pergunta). O link
passa pela barreira de rede (só host público, sem playlist).

**Onde ver.** A nota (`<data hora> - <título>.md`) no `00 Inbox`, com aviso de conteúdo de terceiros.

**Pausar/desligar.** É sob demanda: sem o comando nada acontece.

**Regra.** 43; código em `orion/media_transcribe.py`.

## 15. Documentos e resultados

**O que faz.** Documentos enviados pela interface viram texto pesquisável na memória; arquivos que o Orion
gera (`gerar_documento`, `gerar_imagem`) ficam guardados numa biblioteca com versões.

**Como ligar.** Já vem ligado (é parte da interface).

**Documentos** (tela Conhecimento › Documentos, `POST /memoria/documentos`):
- Formatos: `.txt`, `.md`, `.csv`, `.json`, `.html`, `.htm`, `.docx`, `.xlsx`, `.pdf` (PDF escaneado só
  imagem ainda não entra: precisa de OCR antes).
- Limite: 25 MB por arquivo; até 2 milhões de caracteres indexados. O arquivo não fica guardado: só o texto,
  em trechos.
- **"Disponível em"**: *global* (entra no contexto de qualquer conversa) ou um **projeto** (só nas conversas
  dele). Hoje se escolhe no envio.

**Resultados** (tela Conhecimento › Resultados, `GET /resultados`): cópia em `<dados>/resultados/` com origem (conversa,
projeto, ferramenta); gerar de novo o mesmo nome vira a versão seguinte e as antigas ficam. Prévia de texto
(`.txt`, `.md`, `.csv`, `.json`) e imagem; o resto (HTML, SVG inclusive) só baixa como anexo.

**Dependências.** Nenhuma (a leitura de PDF, Word e Excel usa bibliotecas que já vêm no `uv sync`).

**O que sai do computador.** Nada no envio: a extração de texto é local. Os trechos achados entram no
contexto do chat (vão ao gateway) e, com `ORION_EMBED_API_KEY`, os trechos vão ao Gemini para virar vetores.

**Onde ver.** Tela Conhecimento (cartões Documentos e Resultados), `GET /memoria/documentos` e `GET /resultados`.

**Pausar/desligar.** Apagar: `DELETE /memoria/documentos/{id}` e `DELETE /resultados/{id}` (ou pelos botões).

**Regra.** 11 (limite de upload) e 33 (imagem gerada); código em `orion/app.py` (`/memoria/documentos`,
`/resultados`) e `orion/resultados.py`.

## 16. Tabela-resumo dos opt-ins

Tudo o que liga um recurso, num lugar só. Um teste (`tests/test_operacao_doc.py`) falha se aparecer
opt-in novo sem linha aqui.

| Variável | Liga | Sai do computador? (o quê, para quem) | Regra | Desligar |
|---|---|---|---|---|
| `ORION_DESKTOP_TOOLS` | Agir no computador: comando, arquivos, documentos, clipboard, app, Git, processos, vigilância (§4) | Não por si; o resultado das ferramentas vai ao gateway como contexto | 1, 3, 6, 22, 26 | `false` (padrão) |
| `ORION_WEB_TOOLS` | Busca, leitura de URL, clima, Gemini Search, imagem (§4) | Sim: a busca/URL ao Google, Brave, Open-Meteo e ao site lido | 7, 32, 33 | `false` (padrão) |
| `ORION_VISION_TOOLS` | Capturar e explicar a tela, analisar imagem (§4) | Sim, só com o seu aval: a imagem ao provedor do modelo | 28 | `false` (padrão) |
| `ORION_VOICE_ENABLED` | Falar com o Orion pelo microfone (§4.3 A) | Sim: a fala ao Groq; o texto da resposta à Microsoft (edge-tts) | 35 | `false` (padrão) |
| `ORION_VOICE_LIVE_ENABLED` | Voz ao vivo com o Gemini Live (§4.3 B) | Sim: o áudio do microfone ao Google | 36 | `false` (padrão) |
| `ORION_WAKE_ENABLED` | Palavra de ativação "Orion" (§4.4) | Não antes da palavra; depois, como a voz (Groq, Microsoft) | 38 | `false` (padrão); pausa: `POST /voz/escuta` |
| `ORION_SCREEN_MEMORY` | Memória da tela por OCR local (§8) | Não (só os trechos que o modelo buscar entram no chat) | 44 | `false` (padrão); pausa: `POST /tela/pausa` |
| `ORION_SLEEP_AT` | Ciclo de sono à noite (§9) | Sim: fatos já guardados, uma chamada por noite ao gateway | 42 | vazio (padrão) |
| `ORION_RESEARCH_AT` | Pesquisa noturna (§10) | Sim: os assuntos aos provedores de busca e ao gateway | 39 | vazio (padrão) |
| `ORION_WEEKLY_AI` | Leitura semanal do modelo (§11) | Sim: o resumo semanal, uma vez por semana, ao gateway | 37 | `false` (padrão) |
| `ORION_CONSOLIDATE` | Fatos a partir das suas falas (a cada 6 h) | Sim: as suas falas novas ao gateway | 19 | `true` (padrão) → `false` |
| `ORION_BRIEFING_AT` | Briefing diário (§5.1, §6) | Não sem agenda; com `ORION_BRIEFING_CALENDAR_EMAIL`, a consulta ao Google pelo MCP | 30, 37 | vazio (padrão) |
| `ORION_N8N_WEBHOOKS` | `acionar_n8n` (§12) | Sim, com o seu aval: o corpo JSON ao webhook | 41 | vazio (padrão) |
| `ORION_TELEGRAM_TOKEN` | Canal do celular (§6) | Sim: mensagens e respostas pelo Telegram | 21, 27, 30 | vazio (padrão) |
| `ORION_PLUGINS_ENABLED` | Plugins concedidos (§13) | Os servidores MCP do plugin seguem a §5 | 45 | `true` (padrão) → `false` |
| `ORION_SKILLS_ENABLED` | Skills (§13) | Só nome e descrição no prompt (ao gateway) | 40 | `true` (padrão) → `false` |
| `ORION_MCP_ENABLED` | Servidores do `mcp.json` (§5) | Depende do servidor (e-mail e agenda falam com o Google) | 24, 32 | `true` (padrão) → `false` |
| `ORION_JOBS_ENABLED` | Jobs: lembretes, backup, vault, embeddings, consolidação, briefing | Embeddings ao Gemini (com chave); o resto conforme cada opt-in acima | 18, 20 | `true` (padrão) → `false` |
| `ORION_LOCAL_MODEL` | Modelo local de reserva, sem ferramentas (§17.4) | Não: só `127.0.0.1` (a configuração recusa outro host) | 49 | vazio (padrão) |
| `ORION_DND_AT` | Não perturbe por horário (§17.3) | Não | 48 | vazio (padrão) |
| `ORION_CLAP_ENABLED` | Duas palmas abrem o Orion (§18.2) | Não antes nem depois das palmas; só com `abrir_e_ouvir` a fala seguinte vai como na voz (Groq, Microsoft) | 51 | `false` (padrão); pausa: `POST /voz/escuta` |
| `ORION_ALLOW_PAID` | Aceitar um endereço fora da lista de provedores gratuitos (§17.1) | Para o provedor que você apontou, que **pode cobrar** | 46 | `false` (padrão) |

**Exemplo: ligar a memória da tela só com esta tabela.** Instale o `tesseract` com português (§8), ponha
`ORION_SCREEN_MEMORY=true` no `.env`, reinicie e rode `uv run orion doctor` (deve dizer "memória da tela: OCR
local"). Para pausar, `POST /tela/pausa {"ativa": false}`; para apagar o que guardou, `orion tela --limpar`.

## 17. Custo, privacidade e controle

Regras 46 a 49 ([ORION_REGRAS.md](ORION_REGRAS.md)). Tudo aqui se apoia no **registro de saída**: cada chamada
que leva dado para fora do computador vira uma linha na tabela `external_calls` (provedor, tipo, modelo, ok,
latência, bytes de ida e volta, tipo do conteúdo), **nunca o conteúdo, a URL nem a chave**. Guarda 90 dias,
como o audit.

### 17.1 Custo zero e cota gratuita

**O que faz.** Conta quanto da cota gratuita **o Orion** já usou (por dia; Brave por mês) e, com 90% usado,
segura os jobs opcionais (pesquisa noturna, ciclo de sono, leitura semanal, consolidação) até virar o dia, com
um aviso `cota` por dia. **O chat nunca é bloqueado**; passar de 100% vira aviso no painel.

**Como ligar.** Já vem ligado. Limites (padrão = estimativa do gratuito; 0 = sem limite):
```
# ORION_QUOTA_GATEWAY_DIA=1000
# ORION_QUOTA_GROQ_DIA=2000
# ORION_QUOTA_GEMINI_DIA=1000
# ORION_QUOTA_BRAVE_MES=1000
```
**(você)** Os gratuitos mudam sem aviso (o Google já cortou o do Gemini mais de uma vez): confira no painel de
cada provedor e ajuste. A conta é a do Orion: outro programa com a mesma chave gasta a mesma cota e não aparece aqui.

**Dependências.** Nenhuma.

**O que sai do computador.** Nada além do que cada recurso já manda.

**Onde ver.** Painel › **Cota de hoje** (barra por provedor) e **Provedores** (por provedor e tipo, na semana:
chamadas, falhas, latência p50/p95, modelo mais usado e tráfego). `/painel` no Telegram mostra a cota.

**Pausar/desligar.** Limite `0` desliga a conta daquele provedor.

**Custo zero.** O `orion doctor` avisa se o gateway (ou a transcrição) aponta para um endereço fora da lista de
gratuitos (`127.0.0.1`/`localhost`, `api.groq.com`, `generativelanguage.googleapis.com`, `api.search.brave.com`).
Se o endereço é seu e gratuito (um modelo seu em outra máquina, por exemplo), confirme com `ORION_ALLOW_PAID=true`.
Os hosts do catálogo de provedores já contam como gratuitos; um provedor com `"url"` própria passa pela mesma checagem.

**Regra.** 46; código em `orion/costs.py`.

### 17.2 Modelos fora do ar e painel de privacidade

**O que faz.** Sem chamada extra a provedor nenhum, o job lê o registro de saída: se as últimas 3 chamadas ao
gateway falharam e a primeira delas tem mais de `ORION_GATEWAY_DOWN_MIN` minutos (padrão 10), sai o aviso "Modelos
fora do ar desde HH:MM"; quando uma volta a responder, "Modelos de volta". A tela **Privacidade** (`#/privacidade`,
item na barra lateral e na paleta) mostra, por dia e provedor, quantos envios, quantos bytes e de que tipo (texto,
imagem, áudio), e a lista do que saiu hoje.

**Como ligar.** Já vem ligado (`ORION_GATEWAY_DOWN_MIN=10` ajusta o tempo).

**Dependências.** Nenhuma.

**O que sai do computador.** Nada: é leitura do que já foi registrado. O modelo local não conta como saída.

**Onde ver.** Caixa de atividade (avisos `gateway`), tela Privacidade, `GET /privacidade?dias=7`.

**Pausar/desligar.** O aviso não é urgente: o não perturbe o segura.

**Regra.** 47; código em `orion/saidas.py`, `orion/jobs.py` (`_saude_gateway`), `orion/app.py` (`/privacidade`).

### 17.3 Modo pânico e não perturbe

**O que faz.** **Pânico** é um corte: tira do modelo toda ferramenta que fala com a rede, traz conteúdo de fora,
executa ou apaga; para a memória da tela, a palavra de ativação e os jobs que usam rede (consolidação, sono,
pesquisa, leitura semanal, embeddings, agenda do briefing). O chat continua, só com leitura local e escrita.
**Nada volta sozinho.** **Não perturbe** só segura: os avisos não urgentes ficam na fila (lembrete e agendamento,
que você marcou, são urgentes) e a memória da tela pausa; quando acaba, a fila sai.

**Como ligar.**
```
uv run orion panico                 # liga (o servidor que estiver de pé obedece em segundos)
uv run orion panico --estado
uv run orion panico --sair          # o único jeito pela linha de comando
uv run orion nao-perturbe 07:00     # até a próxima vez que o relógio marcar 07:00
uv run orion nao-perturbe --sair
ORION_DND_AT=22:30-07:00            # todo dia nesse horário
```
No Painel › **Controle**: "Ligar modo pânico" (confirma) e "Sair do modo pânico" (**pede a senha de novo**);
"Não perturbe até…". Na paleta (`Ctrl+K`): "Ligar modo pânico". No Telegram: `/panico`, `/panico sair`,
`/panico estado`. Pela API: `POST /modo/panico {"ativo": true}` (sair: `{"ativo": false, "senha": "..."}`),
`POST /modo/nao-perturbe {"ate": "07:00"}` (ou `null`), `GET /modo`. A tecla global chega com a ponte (E2).

**Dependências.** Nenhuma.

**O que sai do computador.** Nada.

**Onde ver.** Alerta vermelho no topo do Painel, cartão Controle, `orion doctor` e `/painel` no Telegram.
Entrada e saída do pânico ficam no audit (`modo_panico`).

**Pausar/desligar.** `orion panico --sair`, `/panico sair` ou o botão do painel com a senha. Se o audit falhar,
o Orion **não** sai do pânico (entrar nunca é barrado).

**Regra.** 48; código em `orion/modos.py`.

### 17.4 Modelo local de reserva

**O que faz.** Um modelo no próprio computador (Ollama) que só responde quando o gateway falha, **sem
ferramentas** (a requisição sai sem `tools` e com "Modo reserva: sem ferramentas"). Também é o modelo do
auto-compact (E4). Sem gateway configurado, ele sozinho mantém o chat de pé.

**Como ligar.**
```
ORION_LOCAL_MODEL=qwen3.5:4b                 # ou gemma3:4b
# ORION_LOCAL_URL=http://127.0.0.1:11434/v1  # só 127.0.0.1, localhost ou ::1
```

**Dependências.** **(você)** Instalar o [Ollama](https://ollama.com) e rodar `ollama pull qwen3.5:4b`. O
`orion doctor` confere se o Ollama responde e se o modelo está baixado.

**O que sai do computador.** Nada: o endereço só aceita este computador.

**Onde ver.** Painel › Modelos (endpoint `local`, camada `local`) e Provedores (`ollama`); a resposta mostra
`local/<modelo>` como quem respondeu.

**Pausar/desligar.** Esvaziar `ORION_LOCAL_MODEL` e reiniciar.

**Regra.** 49; código em `orion/gateway.py` (`Endpoint.tools`), `orion/app.py` (`gateway_from_settings`).

## 18. Ponte de desktop

Regras 50 e 51 ([ORION_REGRAS.md](ORION_REGRAS.md)). O Orion é servidor + navegador: sem processo à parte não há
tecla global, bandeja nem como colar texto no programa em foco. `orion ponte` é esse processo.

### 18.1 A ponte (bandeja, teclas globais, janela pequena)

**O que faz.** Fica na bandeja do sistema (Abrir Orion · Captura rápida · Modo pânico · Sair), registra as teclas
globais e escuta o servidor por um WebSocket **que ela abre** (`/ws/ponte`). O servidor só consegue mandar duas
coisas: `abrir` (uma tela do Orion no navegador) e `colar` (texto que você já viu e aprovou na tela; a ponte
guarda o que estava na área de transferência, cola e devolve 1 s depois). Teclas padrão: captura rápida
`Ctrl+Alt+Espaço`, "o que é isso?" `Ctrl+Alt+O`, copiar texto de uma área da tela `Ctrl+Alt+T`, pânico
`Ctrl+Alt+Shift+P`.

**Como ligar.** No computador do servidor, uma vez:
```
uv sync --extra ponte                  # pystray, pynput, pyperclip, mss, pytesseract, websockets, pywebview
uv run orion ponte --parear            # cria o token da ponte e o guarda no cofre do sistema (ORION_PONTE_TOKEN)
uv run orion ponte                     # sobe a ponte (o servidor precisa estar de pé)
uv run orion autostart --ponte --install   # início automático da ponte, arquivo à parte do servidor
# ORION_HOTKEYS={"captura": "ctrl+alt+k"}   # troca só as teclas que quiser (JSON)
```
O token tem **escopo `ponte`**: alcança `/captura`, `/ponte/explicar`, o WebSocket e **entrar** no modo pânico;
`/chat`, ferramentas, configuração e sair do pânico respondem 403. Parear de novo desfaz o pareamento anterior, e
trocar a senha também. Só uma ponte conectada por vez.

**Dependências.** **(você)** Sessão gráfica; no Linux, `xclip` ou `xsel` (área de transferência) e libs de
bandeja; no macOS, dar à ponte as permissões de Acessibilidade e Gravação de Tela; no Windows, `tesseract` +
pacote `por` para o OCR (mesmo da memória da tela). A janela pequena usa `pywebview`; sem ele, abre no navegador.

**O que sai do computador.** Nada: a ponte só fala com o servidor local (`127.0.0.1`). O que o servidor faz com a
captura rápida é descrito em §18.3.

**Onde ver.** `GET /ponte/estado` (conectada, desde quando, quantos comandos); o ícone da bandeja.

**Pausar/desligar.** "Sair" na bandeja. O **modo pânico** derruba a ponte e o servidor recusa o token até a saída
do pânico. `orion ponte --parear` de novo, ou trocar a senha, invalida a ponte velha.

**Regra.** 50; código em `orion/ponte/` (`nucleo.py` é testado com adaptadores falsos; `adaptadores.py` é o que
fala com o sistema e **não foi exercitado numa tela real** nesta etapa).

### 18.2 Duas palmas abrem o Orion

**O que faz.** Detecta duas palmas no mesmo microfone da palavra de ativação (§4.4) e abre o Orion. Uma palma é um
**pico curto**: acima de `ORION_CLAP_RATIO` vezes o ruído de fundo (e de um piso absoluto), e o quadro seguinte
cai para menos de 40% da energia. Porta batendo, móvel arrastado e música têm cauda e não contam; digitação fica
abaixo do piso. Duas palmas com 150 a 700 ms entre elas disparam; uma terceira dentro de 700 ms é aplauso ou
batida: cancela e o detector fica surdo por 1,5 s. **Palma só abre** (regra 51): nunca aprova ação pendente e nunca chama
ferramenta.

**Como ligar.**
```
ORION_CLAP_ENABLED=true
# ORION_CLAP_RATIO=6.0            # suba se aplaudir/falar alto dispara à toa; desça se não pega
# ORION_CLAP_ACTION=abrir         # abrir | abrir_e_ouvir | rotina:<nome> (a rotina só existe a partir da E9.1)
# ORION_CLAP_MAX_PER_HOUR=20
```
`abrir` manda `abrir` à ponte (§18.1) ou, sem ponte, abre `http://127.0.0.1:<porta>/ui/#/chat` no navegador.
`abrir_e_ouvir` também começa um turno de voz, como a palavra de ativação (precisa de `ORION_VOICE_ENABLED` e da chave
de transcrição). **(você)** Calibrar no quarto: o Painel › Voz mostra quantas palmas acionaram e a força (pico ÷ ruído) das
últimas detectadas na última hora; ajuste `ORION_CLAP_RATIO` até pegar as suas e ignorar o resto.

**Dependências.** **(você)** `pip install "orion[wake]"` (ou `wake-vosk`): só o `sounddevice` e o PortAudio importam
aqui; o detector de palmas não usa modelo.

**O que sai do computador.** Nada: o áudio existe só no quadro de 80 ms que está sendo analisado, nunca é gravado
nem enviado. Com `abrir_e_ouvir`, a fala **depois** das palmas segue o caminho da voz (Groq para a transcrição, Microsoft
para a resposta falada).

**Onde ver.** Painel › Voz (linha "Duas palmas"), audit (`voz_palmas`, `voz_palmas_recusadas`: só o horário e o motivo),
`orion doctor`.

**Pausar/desligar.** `ORION_CLAP_ENABLED=false`; ou `POST /voz/escuta {"ativa": false}` (pausa a escuta inteira); o
**modo pânico** e o **não perturbe** seguram as palmas (nada acontece, e fica no audit); teto por hora em
`ORION_CLAP_MAX_PER_HOUR`.

**Regra.** 51; código em `orion/palmas.py` (detector, provado com sinais sintéticos: duas palmas, três, porta batendo,
digitação, ruído alto) e `orion/wake.py` (`WakeListener` divide o microfone entre palavra e palmas).
