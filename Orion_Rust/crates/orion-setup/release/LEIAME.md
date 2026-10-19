# Chave do release

O instalador baixa o app do GitHub Releases e só aceita arquivos cujo feed (`latest-cpu.json` /
`latest-vulkan.json`) tenha assinatura minisign válida desta chave.

1. O par já foi gerado (sem senha) e `minisign.pub` desta pasta já é a chave pública. Para trocar a chave: `minisign -G -p minisign.pub -s minisign.key`.
2. (feito) A chave pública está commitada aqui.
3. No GitHub: Settings → Secrets → `MINISIGN_SECRET_KEY` (conteúdo de `minisign.key`) e, se a chave tiver
   senha, `MINISIGN_PASSWORD`. Guarde `minisign.key` fora do repositório.
4. Publique: `git tag v0.1.0 && git push origin v0.1.0` (ou rode o workflow "Release Windows").

Com `minisign.pub` vazio o instalador não baixa nada (só usa um pacote que esteja ao lado dele).
