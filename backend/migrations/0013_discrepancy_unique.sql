-- Every reconcile pass re-INSERTed the same discrepancies as new rows.
-- Rebuild the table with a natural unique key so re-flagging updates in
-- place. `period` becomes NOT NULL DEFAULT '' because SQLite treats NULLs
-- as distinct inside UNIQUE constraints.
CREATE TABLE discrepancies_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    field      TEXT NOT NULL,
    period     TEXT NOT NULL DEFAULT '',
    source_a   TEXT NOT NULL,
    value_a    REAL NOT NULL,
    source_b   TEXT NOT NULL,
    value_b    REAL NOT NULL,
    pct_diff   REAL NOT NULL,
    flagged_at TEXT NOT NULL,
    UNIQUE (company_id, field, period, source_a, source_b)
);

-- Copy old rows in id order so the latest duplicate wins the unique slot.
INSERT INTO discrepancies_new (company_id,field,period,source_a,value_a,source_b,value_b,pct_diff,flagged_at)
SELECT company_id, field, COALESCE(period,''), source_a, value_a, source_b, value_b, pct_diff, flagged_at
FROM discrepancies ORDER BY id
ON CONFLICT (company_id,field,period,source_a,source_b) DO UPDATE SET
    value_a=excluded.value_a, value_b=excluded.value_b,
    pct_diff=excluded.pct_diff, flagged_at=excluded.flagged_at;

DROP TABLE discrepancies;
ALTER TABLE discrepancies_new RENAME TO discrepancies;
