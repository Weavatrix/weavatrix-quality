ALTER TABLE program_revisions ADD COLUMN change_id TEXT NOT NULL DEFAULT '';
ALTER TABLE program_revisions ADD COLUMN repository_revision TEXT NOT NULL DEFAULT '';
ALTER TABLE program_revisions ADD COLUMN body_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE program_revisions ADD COLUMN source TEXT NOT NULL DEFAULT 'healed';
ALTER TABLE program_revisions ADD COLUMN preview_id TEXT;
ALTER TABLE program_revisions ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS authoring_previews (
    id TEXT PRIMARY KEY,
    program TEXT NOT NULL,
    change_id TEXT NOT NULL,
    repository_revision TEXT NOT NULL,
    seal TEXT NOT NULL,
    program_hash TEXT NOT NULL,
    passed INTEGER NOT NULL CHECK (passed IN (0, 1)),
    promoted_revision INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_authoring_previews_program
    ON authoring_previews(program, created_at);
