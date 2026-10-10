# Agenda e conector Google Calendar

Conector de referência: [nspady/google-calendar-mcp](https://github.com/nspady/google-calendar-mcp),
pacote @cocal/google-calendar-mcp **2.7.0**, MIT, Node >=20, SDK Node ^1.30.1.
Pesquisa em 07/10/2026: repositório não arquivado, atualização 01/10/2026; pacote publicado e
integridade conferidos. Instalado para ensaio com scripts npm desativados, fora do runtime Orion.

A ponte consultar_agenda/consultar_disponibilidade usa apenas list-events e get-freebusy.
Conta explícita, calendário e IANA timezone são revisados em Integrações > Conexões MCP > Agenda.
Nunca omite account: o upstream mescla contas quando account está ausente. Classificação da
conexão deve conter apenas ferramentas de leitura; manage-accounts é recusado pelo Orion.
O upstream anuncia essa ferramenta mesmo com --enable-tools de leitura: filtros sozinhos
não bastam. Alterar configuração da conexão invalida o vínculo; reinício mantém conexão desligada.

Período exige offsets explícitos, duração positiva até 31 dias, retorno até 32 KB. Resultado
inclui conector, conexão, conta, calendário, fuso, revisão e geração. Conteúdo é externo e não
concede permissão. Cota, timeout, conexão desligada/revogada e incompatibilidade são acionáveis;
chamadas não são repetidas automaticamente.

## Configuração e limites reais

A autenticação Google acontece no servidor Calendar, separada do OAuth MCP do Orion. O conector
usa arquivos próprios de cliente/tokens e seu consentimento padrão inclui calendar amplo;
restringir tools no Orion **não reduz o consentimento Google**. Por isso a validação com conta
real e adequação de escopos mínimos continuam pendentes. Não houve login, acesso à agenda
pessoal ou arquivo real de credenciais; somente credenciais claramente fictícias no ensaio.

Produção local: executar servidor externo de confiança previamente instalado e revisado,
usando executable/argv absolutos no stdio do Orion; sem instalar dependências silenciosamente.
Segredos ficam fora de bundles/plugins. O HTTP upstream é só para loopback de ensaio,
sem autenticação própria; Orion recusa vincular HTTP remoto sem Bearer/OAuth explícito.
O Orion não administra nem declara proteger no seu cofre os arquivos Google do upstream.

## Evidências

O pacote publicado real aceitou inicialização e list_tools pelo SDK Python 2.3.0, negociando
MCP 2025-11-25. Descoberta: list-events, get-freebusy, get-current-time e manage-accounts
(fora da allowlist). Esquemas de leitura reais preservados como fixture; nenhum call_tool no
Google real. Fixtures de consulta cobrem fuso -03:00, conta, dois projetos, cota, timeout,
conexão indisponível e ausência de chamadas de escrita. Isso prova protocolo e contratos,
sem declarar consulta real autorizada ou compatibilidade com desktops não executados.

## Orion Agenda — propostas e criação revisada (C42)

Pacote com planejar-dia e preparar-reuniao, acesso a consultar_agenda,
consultar_disponibilidade e propor_evento_agenda. O modelo só prepara uma proposta
persistida e imutável; salvar essa proposta também respeita a política da conversa.
A criação real não aparece no catálogo do modelo. Em Agenda, o usuário confere título,
conta, calendário, início/fim, fuso, descrição e local antes de pedir aprovação e confirmar.
Participantes/convites não são aceitos; sendUpdates=none e eventId estável por proposta.

A decisão se vincula a sessão/projeto/revisão, digest do conteúdo, configuração do conector,
geração da conexão e revisão da ferramenta create-event descoberta. Alterar qualquer um
revoga a validade do envio. A rejeição não chama o conector. O canal de chat direciona a
revisão para Agenda; não consome essa decisão em /chat/resume.

Para criar, o operador precisa disponibilizar create-event no servidor revisado, mantendo
essa ferramenta fora da classificação/catálogo LLM do Orion. O consentimento Google amplo
continua uma limitação do upstream, pendente no C41; o pacote não declara autorização real.
Timeout/cancelamento grava resultado unknown e impede replay. O usuário deve conferir a
agenda antes de propor outra criação; não há retry silencioso. Reinício preserva propostas
mas não retoma envios. Versões/remoção de plugin e revogação não apagam a conversa.
