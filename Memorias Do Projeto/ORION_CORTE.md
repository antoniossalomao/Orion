# ORION — Plano de corte (venda do PC)

> Como sair do legado sem perder dados nem ficar sem assistente. Criado em 03/10/2026.
> Os critérios e prazos abaixo são **proposta**; o que depende de você está marcado **(você)**.
> Fases e decisões gerais: [ORION_NUCLEO.md](ORION_NUCLEO.md) §6–§7.

## 1. Por que existe

O legado só roda no PC atual (GPU, Qdrant, SurrealDB). O notebook de 8 GB não roda nada disso, e a
reescrita (`orion/`) ainda não tem Telegram novo, login, ferramentas MCP nem modelos reais ligados.
Vender o PC é o **ponto sem volta**: depois dele, voltar ao legado é reconstruir, não religar. Por isso
a venda só acontece com os dois blocos abaixo cumpridos.

| Bloco | O que garante | Situação em 03/10/2026 |
|---|---|---|
| **A. Dados salvos e provados** | Nada do que importa fica só no PC | Falta executar (é no PC); a verificação existe: `orion verify-export` |
| **B. Orion mínimo no notebook** | Há assistente depois da venda | Falta ligar: modelos reais, canal móvel, ferramentas mínimas |

Data da venda: ______ **(você)**. Se o bloco B não ficar pronto antes dela, as saídas são adiar a venda
ou passar um período só com o export guardado e sem assistente; a segunda não é recomendada.

## 2. Bloco A — dados (no PC atual, com o legado no ar)

1. No legado, rodar a ferramenta `backup_memoria` (ela exporta **todas** as tabelas do SurrealDB, grafo
   incluído, para `<tabela>.json`). Os arquivos de vetores do Qdrant não importam: os embeddings são refeitos.
2. Copiar para fora do PC: `.env`, `Orion_Core/google_auth/` e o vault do Obsidian.
3. Conferir tudo de uma vez (não toca no banco real):

   ```
   uv run orion verify-export <pasta-do-backup> --assistente <nome-antigo-do-assistente> \
       --env <.env copiado> --google-auth <google_auth copiada> --vault <vault copiado>
   ```

   Pronto quando a última linha for `RESULTADO: PRONTO`. O relatório mostra, por tabela, quantos
   registros há no export e quantos a importação de ensaio contou (novos, repetidos, inválidos), faz
   backup e restauração do banco de ensaio e busca uma fala importada no banco restaurado. De `.env`
   mostra só os **nomes** das variáveis, nunca os valores.
4. Se o relatório avisar "atores que viram 'system'", é um nome antigo do assistente: repita com
   `--assistente <nome>`. Sem isso, as respostas antigas não aparecem na busca de conversas.
5. Guardar o export em dois lugares (nuvem e disco externo). Ele contém conversas inteiras: tratar como
   dado pessoal.

## 3. Bloco B — Orion mínimo (no notebook)

Critérios para considerar que o legado pode sair:

| # | Critério | Como se prova | Situação |
|---|---|---|---|
| 1 | Memória importada e pesquisável | `orion import-surreal <pasta> --assistente <nome>`; contagens iguais às do `verify-export` | código pronto |
| 2 | Busca acerta as suas perguntas | 15–30 perguntas em `tests/eval_pessoal.local.json` (modelo: `tests/eval_pessoal.example.json`); `python -m orion.memory.eval <casos> --db <orion.db> --embeddings --min-hit-rate 0.8` | **(você)** escrever as perguntas; chave de embeddings |
| 3 | Modelos reais respondem | OmniRoute no ar; `POST /chat` segue respondendo com um provedor derrubado de propósito | **(você)** subir o OmniRoute |
| 4 | Canal no celular | Telegram lendo o `/chat` novo e entregando `/notifications`. Caminho mais curto: o bot do legado (`orion_telegram.py`) já lê o `/chat` novo sem mudança (ORION_MELHORIAS B1); a verificar no notebook. O bot novo com botões de aprovar é a fase 5 | a verificar / fase 5 |
| 5 | Acesso de fora sem expor porta | Tailscale; `ORION_ALLOWED_HOSTS` com o nome do notebook **exige** `ORION_ADMIN_TOKEN` (a configuração recusa subir sem) | código pronto; **(você)** instalar Tailscale |
| 6 | Backup restaurável | `ORION_BACKUP_DIR` apontando para o iCloud/OneDrive; um backup restaurado com `orion restore` num banco limpo | código pronto |
| 7 | Ferramentas essenciais | Lembretes, tarefas e memória já existem; `delegar` com as CLIs reais; o resto da [triagem](ORION_FERRAMENTAS.md) que você exigir antes da venda: ______ **(você)** | parcial |

Período em paralelo antes de vender: proposta de **7 dias** usando só o Orion novo no dia a dia, com o
legado desligado mas intacto. Os dois não sobem juntos (a porta 8000 é a mesma; use `ORION_PORT`).

## 4. Corte

1. Um segundo export final (a importação é idempotente: repetir não duplica) e `verify-export` de novo.
2. Checklist: bloco A verde, critérios 1–7 verdes, 7 dias de uso sem perda.
3. Só então vender. Fase 7 (apagar `Orion_Ollama/`, `Orion_Core/`, `bin/`) vem **depois**, não antes.

## 5. Rollback

| Quando | O que volta | Como |
|---|---|---|
| Antes da venda | Tudo | Religar o legado: nada foi apagado, o `orion/` roda ao lado |
| Depois da venda | Dados | `orion restore <backup>` num banco limpo, ou `orion import-surreal` do export guardado |
| Depois da venda | O legado | Não há "religar": reconstruir a partir do repositório `Lyra` (backup do legado) e do export. Por isso a venda é o ponto sem volta |

## 6. O que falta decidir **(você)**

| Decisão | Recomendação |
|---|---|
| Data da venda | Só depois do bloco B; sem data fixa antes disso |
| Dias em paralelo | 7 |
| Nome(s) antigo(s) do assistente | Os que o `verify-export` listar em "atores que viram system" |
| Consolidar o histórico importado em fatos | Depois da venda, em lotes (consome a cota gratuita do modelo); por padrão a consolidação ignora o canal `legado` |
| Onde guardar os backups | Pasta sincronizada do iCloud/OneDrive (`ORION_BACKUP_DIR`) |
| Agendamento que executa ferramenta sozinho | Não por enquanto: o disparo só avisa (regra 18 do [ORION_REGRAS.md](ORION_REGRAS.md)) |
