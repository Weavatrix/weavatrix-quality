ALTER TABLE manual_sessions ADD COLUMN repository_revision TEXT NOT NULL DEFAULT '';
ALTER TABLE manual_sessions ADD COLUMN trace_hash TEXT;
ALTER TABLE manual_sessions ADD COLUMN preview_id TEXT;

CREATE TABLE IF NOT EXISTS manual_session_obligations (
    session TEXT NOT NULL,
    obligation TEXT NOT NULL,
    PRIMARY KEY (session, obligation),
    FOREIGN KEY (session) REFERENCES manual_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS manual_session_api_operations (
    session TEXT NOT NULL,
    operation TEXT NOT NULL,
    PRIMARY KEY (session, operation),
    FOREIGN KEY (session) REFERENCES manual_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_manual_session_obligations_obligation
    ON manual_session_obligations(obligation);
CREATE INDEX IF NOT EXISTS idx_manual_session_api_operations_operation
    ON manual_session_api_operations(operation);
