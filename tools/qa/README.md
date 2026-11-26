# QA de usuário do Orion (Windows)

Scripts que a skill `orion-teste-usuario` chama. Janela sempre visível, UI Automation (pywinauto), um PNG por passo.

## Uso
```powershell
cd tools\qa
.\run_qa.ps1 -Area chat,janela -Modo relatorio -Docx C:\chaves.docx
.\run_qa.ps1 -Area todas -Modo corrigir -Setup "C:\...\Orion Setup.exe" -Bin Orion_Rust\target\release
```
- Áreas: `instalador, onboarding, login, janela, chat, config, projetos, maestro, mini, bandeja` (ou `todas`).
- `-Modo`: só rotula o JSON (`relatorio`/`corrigir`); os scripts nunca alteram código.
- Saída: última linha = caminho de `qa-prints/<data>/resultado.json` (falhas com `print` e `erro`). Exit 0 ok, 1 falhas, 2 uso.
- Prints: `qa-prints/<data>/<area>/NN-passo.png`; log por passo em `log.txt` (passou/falhou/pulou).
- Claude deve abrir **só** os prints listados em `prints_das_falhas`.

## Segurança
- Perfil isolado em `tools/qa/.perfil/` (`ORION_DATA_DIR`, `ORION_VAULT_DIR`, porta 18765). Nunca o perfil real. `-Limpar` apaga.
- Admin de teste: `Orion_Rust/.orion-test-admin.local` (usuário `qa-admin`, senha aleatória de 32 caracteres, gerado na 1ª execução, ACL só do usuário). No `.gitignore`.
- Chaves de API: lidas do `.docx` (linhas `NOME: valor` ou `NOME=valor`; `claude`→`anthropic`, `google`→`gemini`), só em memória, repassadas por `ORION_KEY_<ID>` ao processo de teste. Log/JSON/erros passam por mascarador; o JSON lista só os nomes das variáveis. A área `config` não abre campos de chave (um print não deve mostrar segredo).

## Limites conhecidos (nada disto foi executado: sem Windows no ambiente de criação)
- Validado só: sintaxe Python (`compileall`), `ruff`, e lógica de mascarar/docx/JSON em Linux. **Não rodou em Windows.**
- UAC roda na área segura: sem Administrador, `instalador` pula esses passos (use desktop use) ou rode o runner elevado.
- iced expõe pouca árvore de acessibilidade: seletores por texto (`clicar_texto`) e fluxos por teclado são palpites em PT-BR; ajuste os textos em `areas/*.py` na 1ª execução real.
- Flags assumidas e não confirmadas: `ORION_PORT`, `orion serve`, `orion set-password --stdin` (existe no código), campos de login (usuário→Tab→senha→Enter).
- Fora do escopo: Modelos, Agente, Vault, Voz/Câmera/Telegram/MCP, desempenho (a skill cobre com desktop use).
