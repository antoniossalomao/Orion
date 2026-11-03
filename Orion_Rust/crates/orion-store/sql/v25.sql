-- Tokens reais informados pelo provedor (campo `usage`); -1 = o provedor não informou (o painel estima por bytes).
ALTER TABLE external_calls ADD COLUMN tokens_in INTEGER NOT NULL DEFAULT -1;
ALTER TABLE external_calls ADD COLUMN tokens_out INTEGER NOT NULL DEFAULT -1;
