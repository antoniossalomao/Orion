# Documentos e fontes no Orion

A tela Fontes recebe PDF com texto, Markdown e TXT UTF-8. O usuário seleciona contexto
pessoal ou projeto antes de enviar; o original e o texto indexado permanecem nesse escopo.
Conteúdo de documentos é fonte externa: não autoriza ferramentas ou mudanças de configuração.

Limites: 6 MiB por original, 256 MiB de originais, 200 páginas PDF, 2 MiB de texto extraído,
dois extratores simultâneos e deadline de 12 segundos. O extrator roda em processo descartável,
com ambiente sem credenciais; em POSIX também recebe limites de memória e CPU. Isso não é
uma sandbox do SO para programas arbitrários. O parser fixado é pypdf 6.19.0, BSD-3-Clause.

PDF criptografado, inválido e sem texto recebem erro específico. OCR não está implementado.
Arquivo com falha continua no banco e pode ser baixado ou reprocessado; interrupção no meio
não perde o original. Upload recusado por tamanho/tipo conserva a seleção no frontend.
O rascunho da conversa não é alterado. Progresso indeterminado indica processamento real,
sem porcentagens inventadas. Busca e embeddings usam o índice existente por projeto.
