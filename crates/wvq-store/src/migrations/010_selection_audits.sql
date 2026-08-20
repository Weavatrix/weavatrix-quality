CREATE TABLE selection_audits (
    id TEXT PRIMARY KEY,
    impacted_run TEXT NOT NULL,
    full_run TEXT NOT NULL,
    change_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('corroborated', 'contradicted', 'unmeasured', 'not_reduced')),
    missed_failures INTEGER NOT NULL,
    learned_tests INTEGER NOT NULL,
    UNIQUE (impacted_run, full_run),
    FOREIGN KEY (impacted_run) REFERENCES runs(id),
    FOREIGN KEY (full_run) REFERENCES runs(id)
);

CREATE TABLE selection_miss_observations (
    test_path TEXT NOT NULL,
    node_id TEXT NOT NULL,
    audit_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    PRIMARY KEY (test_path, node_id, audit_id),
    FOREIGN KEY (audit_id) REFERENCES selection_audits(id)
);

CREATE INDEX selection_miss_observations_node
ON selection_miss_observations(node_id, test_path);
