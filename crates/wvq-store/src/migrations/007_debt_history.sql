CREATE TABLE debt_history (
    fingerprint TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('fixed')),
    revision TEXT NOT NULL
);
