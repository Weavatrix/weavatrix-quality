CREATE TABLE test_case_results (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    executor TEXT NOT NULL,
    suite TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pass', 'fail', 'skip', 'error')),
    duration_ms INTEGER,
    fingerprint TEXT,
    FOREIGN KEY (run_id) REFERENCES runs(id),
    FOREIGN KEY (fingerprint) REFERENCES failure_fingerprints(id)
);

CREATE INDEX test_case_results_identity
ON test_case_results(executor, suite, name);

CREATE INDEX test_case_results_run
ON test_case_results(run_id);
