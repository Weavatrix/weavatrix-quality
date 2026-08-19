ALTER TABLE behavior_edges ADD COLUMN action TEXT NOT NULL DEFAULT '';
ALTER TABLE manual_sessions ADD COLUMN seed INTEGER;
ALTER TABLE manual_sessions ADD COLUMN fixture TEXT;

CREATE TABLE IF NOT EXISTS session_events (
    session TEXT NOT NULL,
    seq INTEGER NOT NULL,
    action TEXT NOT NULL,
    state_digest TEXT NOT NULL,
    PRIMARY KEY (session, seq)
);
