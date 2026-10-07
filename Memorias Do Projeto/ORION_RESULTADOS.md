# Resultados persistentes

C34 armazena texto, Markdown, código e imagens no SQLite, com IDs opacos, conversa,
projeto, versão, digest e fonte da mensagem original. Conteúdo não depende da pasta de
um plugin e a remoção do pacote preserva o resultado. Downloads exigem autenticação e
escopo explícito; títulos não aceitam caminhos. Versões anteriores são imutáveis e
atualizações exigem a versão atual, evitando sobrescrever uma edição concorrente.

Limites: 4 MiB por conteúdo normalizado, 6 MiB por pedido JSON, 50 versões por resultado,
1.000 resultados e 256 MiB de conteúdo total. Imagens PNG/JPEG/WebP são decodificadas e
regravadas em PNG sem metadados; limite de 16 milhões de pixels e 8.192 px por dimensão.
SVG, formatos executáveis e conteúdo inválido são recusados. Pillow **12.3.0**, licença
**MIT-CMU**, confirmado no [PyPI](https://pypi.org/project/Pillow/12.3.0/), fixado no lock.

Prévia/download conferem o digest. Downloads usam attachment, nosniff e CSP restritiva;
nenhum caminho recebido do navegador é aberto no sistema de arquivos. Associar uma
conversa a outro projeto não muda silenciosamente o escopo de resultados já produzidos.
Arquivamento mantém consulta e download, mas impede gravação em conversas arquivadas.

C35 adiciona a biblioteca pesquisável por título, filtrada pelo contexto pessoal/projeto.
Cada prévia mostra título, tipo, conversa, versão e fontes, com download autenticado e
ligação à mensagem original. Texto/código usam DOM de texto; Markdown usa o renderizador
seguro do Orion com imagens incorporadas removidas antes da inserção no DOM. Imagens
normalizadas usam blob local autorizado e a URL é revogada ao fechar/sair/trocar contexto.
Prévia textual até 120 mil caracteres; download mantém o conteúdo completo.

O botão Salvar resultado do chat recebe IDs da mensagem e sessão no encerramento do SSE,
permitindo salvar inclusive a primeira resposta antes da atualização da sidebar. O conteúdo
pode ser revisado antes de salvar; versões anteriores permanecem. Fechar devolve foco à lista,
sair e voltar preserva a versão escolhida e janelas estreitas empilham lista e painel.
Atalhos anteriores Alt+1…5 são mantidos; Alt+6 abre Projetos e Alt+7 abre Resultados.
