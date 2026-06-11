-- Ratios gain a period_type so quarterly and annual figures for the same
-- period_end (e.g. a Q4 end == fiscal-year end) no longer collide. SQLite can't
-- alter a UNIQUE constraint in place, so rebuild the table; existing rows are
-- pre-period-type and are treated as annual.
CREATE TABLE ratios_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id  INTEGER NOT NULL REFERENCES companies(id),
    period_end  TEXT NOT NULL,
    period_type TEXT NOT NULL,   -- quarterly | annual
    metric      TEXT NOT NULL,
    value       REAL NOT NULL,
    computed_at TEXT NOT NULL,
    UNIQUE (company_id, period_end, period_type, metric)
);

INSERT INTO ratios_new (company_id, period_end, period_type, metric, value, computed_at)
    SELECT company_id, period_end, 'annual', metric, value, computed_at FROM ratios;

DROP TABLE ratios;
ALTER TABLE ratios_new RENAME TO ratios;

CREATE INDEX idx_ratios_company_period ON ratios (company_id, period_type);
