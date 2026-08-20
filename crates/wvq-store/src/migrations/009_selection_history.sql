CREATE TABLE test_node_observations (
    test_path TEXT NOT NULL,
    node_id TEXT NOT NULL,
    observations INTEGER NOT NULL CHECK (observations > 0),
    last_revision TEXT NOT NULL,
    PRIMARY KEY (test_path, node_id)
);

CREATE INDEX test_node_observations_node
ON test_node_observations(node_id, observations);

CREATE TABLE test_node_observation_runs (
    test_path TEXT NOT NULL,
    node_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    PRIMARY KEY (test_path, node_id, run_id),
    FOREIGN KEY (run_id) REFERENCES runs(id)
);
