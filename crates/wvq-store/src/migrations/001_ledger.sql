CREATE TABLE repositories (
    id TEXT PRIMARY KEY,
    root TEXT NOT NULL
);

CREATE TABLE revisions (
    id TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    FOREIGN KEY (repository) REFERENCES repositories(id)
);

CREATE TABLE changes (
    id TEXT PRIMARY KEY,
    revision TEXT
);

CREATE TABLE requirements (
    id TEXT PRIMARY KEY,
    change_id TEXT
);

CREATE TABLE scenarios (
    id TEXT PRIMARY KEY,
    requirement TEXT
);

CREATE TABLE obligations (
    id TEXT PRIMARY KEY,
    scenario TEXT
);

CREATE TABLE oracle_seals (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL
);

CREATE TABLE quality_policies (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL
);

CREATE TABLE test_programs (
    id TEXT PRIMARY KEY
);

CREATE TABLE program_obligations (
    program TEXT NOT NULL,
    obligation TEXT NOT NULL,
    PRIMARY KEY (program, obligation)
);

CREATE TABLE executors (
    id TEXT PRIMARY KEY
);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    executor TEXT
);

CREATE TABLE run_items (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL
);

CREATE TABLE observations (
    id TEXT PRIMARY KEY,
    run_id TEXT
);

CREATE TABLE behavior_states (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL
);

CREATE TABLE behavior_edges (
    id TEXT PRIMARY KEY,
    src TEXT NOT NULL,
    dst TEXT NOT NULL
);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    byte_len INTEGER NOT NULL
);

CREATE TABLE proofs (
    id TEXT PRIMARY KEY,
    revision TEXT NOT NULL,
    obligation TEXT NOT NULL,
    oracle_seal TEXT NOT NULL,
    verdict TEXT NOT NULL,
    program TEXT,
    run_id TEXT
);

CREATE TABLE proof_artifacts (
    proof TEXT NOT NULL,
    artifact TEXT NOT NULL,
    PRIMARY KEY (proof, artifact),
    FOREIGN KEY (proof) REFERENCES proofs(id),
    FOREIGN KEY (artifact) REFERENCES artifacts(id)
);

CREATE TABLE quality_findings (
    id TEXT PRIMARY KEY,
    check_id TEXT NOT NULL
);

CREATE TABLE debt_fingerprints (
    fingerprint TEXT PRIMARY KEY
);

CREATE TABLE debt_baselines (
    id TEXT PRIMARY KEY
);

CREATE TABLE coverage_regions (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL
);

CREATE TABLE test_coverage_bitmaps (
    id TEXT PRIMARY KEY,
    artifact TEXT
);

CREATE TABLE behavior_coverage_bitmaps (
    id TEXT PRIMARY KEY,
    artifact TEXT
);

CREATE TABLE failure_fingerprints (
    id TEXT PRIMARY KEY
);

CREATE TABLE failure_occurrences (
    id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL
);

CREATE TABLE mutation_cases (
    id TEXT PRIMARY KEY
);

CREATE TABLE mutation_results (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL
);

CREATE TABLE manual_sessions (
    id TEXT PRIMARY KEY
);

CREATE TABLE ai_usage (
    id TEXT PRIMARY KEY,
    tokens INTEGER NOT NULL
);

CREATE TABLE human_decisions (
    id TEXT PRIMARY KEY
);

CREATE TRIGGER proofs_no_update
BEFORE UPDATE ON proofs
BEGIN
    SELECT RAISE(ABORT, 'proofs are immutable');
END;

CREATE TRIGGER proofs_no_delete
BEFORE DELETE ON proofs
BEGIN
    SELECT RAISE(ABORT, 'proofs are immutable');
END;
