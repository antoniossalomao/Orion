CREATE TABLE oauth_accounts(id TEXT PRIMARY KEY,name TEXT NOT NULL,url TEXT NOT NULL,
 scope TEXT NOT NULL,scopes TEXT NOT NULL,granted TEXT NOT NULL,revision TEXT NOT NULL,
 state TEXT NOT NULL,expires_at REAL,error TEXT);
