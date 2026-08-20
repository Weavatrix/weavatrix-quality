ALTER TABLE runs ADD COLUMN change_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN revision TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN status TEXT NOT NULL DEFAULT 'complete';
ALTER TABLE runs ADD COLUMN passed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN outcome TEXT NOT NULL DEFAULT 'failed';

ALTER TABLE run_items ADD COLUMN executor TEXT NOT NULL DEFAULT '';
ALTER TABLE run_items ADD COLUMN status_code INTEGER;
ALTER TABLE run_items ADD COLUMN passed INTEGER NOT NULL DEFAULT 0;

CREATE TABLE run_artifacts (
    run_id TEXT NOT NULL,
    artifact TEXT NOT NULL,
    PRIMARY KEY (run_id, artifact),
    FOREIGN KEY (run_id) REFERENCES runs(id),
    FOREIGN KEY (artifact) REFERENCES artifacts(id)
);

CREATE INDEX runs_change_revision ON runs(change_id, revision);
