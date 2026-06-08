-- Persisted Graham defensive-investor summary per company, for screening.
CREATE TABLE graham_scores (
    company_id       INTEGER PRIMARY KEY REFERENCES companies(id),
    score            INTEGER NOT NULL,
    passes_defensive INTEGER NOT NULL,
    graham_number    REAL,
    ncav_per_share   REAL,
    margin_of_safety REAL,
    net_net          INTEGER NOT NULL,
    computed_at      TEXT NOT NULL
);
CREATE INDEX idx_graham_score ON graham_scores (score DESC);
