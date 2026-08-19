ALTER TABLE failure_fingerprints ADD COLUMN digest TEXT NOT NULL DEFAULT '';
ALTER TABLE failure_fingerprints ADD COLUMN class TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE failure_occurrences ADD COLUMN seen_at TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS program_revisions (
    program TEXT NOT NULL,
    revision INTEGER NOT NULL,
    seal TEXT NOT NULL,
    PRIMARY KEY (program, revision)
);
