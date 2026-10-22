# ADR 0002 — Banco SQLite com SQLCipher

**Estado:** aceita

## Contexto
Os dados pessoais precisam ficar cifrados em disco.

## Decisão
SQLite com SQLCipher (opt-in por `orion db-cifrar`), chave mestra embrulhada pela senha (Argon2id) e por um código de recuperação. O sync entre PCs usa um log de eventos próprio; cr-sqlite saiu da solução (20/10/2026).

## Consequências
Senha nova usa Argon2id; hash PBKDF2 antigo migra no primeiro login.
