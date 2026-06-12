-- Per-company, per-source collection failures. The collect summary only
-- counted these; persisting them lets the API answer "which source failed
-- for this ticker, and why".
CREATE TABLE source_errors (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id  INTEGER NOT NULL REFERENCES companies(id),
    source      TEXT NOT NULL,
    message     TEXT NOT NULL,
    occurred_at TEXT NOT NULL
);

CREATE INDEX idx_source_errors_company_time ON source_errors (company_id, occurred_at);
