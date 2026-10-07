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
