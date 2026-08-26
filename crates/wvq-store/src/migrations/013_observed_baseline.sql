CREATE TABLE observed_debt_baselines (
    fingerprint TEXT PRIMARY KEY NOT NULL,
    revision TEXT NOT NULL,
    change_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision = 'observed_only')
);
