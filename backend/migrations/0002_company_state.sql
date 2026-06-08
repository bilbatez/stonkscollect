-- Tracks the last time each company's fundamentals were collected, so bulk
-- passes can skip recently-collected companies (incremental collection).
CREATE TABLE company_state (
    company_id        INTEGER PRIMARY KEY REFERENCES companies(id),
    last_collected_at TEXT NOT NULL
);
