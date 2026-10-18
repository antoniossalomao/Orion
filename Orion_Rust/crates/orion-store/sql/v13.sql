CREATE TABLE calendar_bindings(scope TEXT PRIMARY KEY,connection_id TEXT NOT NULL,
 account TEXT NOT NULL,calendar_id TEXT NOT NULL,timezone TEXT NOT NULL,revision TEXT NOT NULL,
 connection_revision TEXT NOT NULL);
