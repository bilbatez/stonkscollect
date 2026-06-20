-- Per-user preferences: UI theme + Graham defensive thresholds. A missing row
-- (or a NULL column) means "use the default". One row per user.
CREATE TABLE user_settings (
    user_id            INTEGER PRIMARY KEY REFERENCES users(id),
    theme              TEXT NOT NULL DEFAULT 'system', -- system | light | dark
    graham_min_revenue REAL,
    pe_max             REAL,
    pb_max             REAL,
    pe_pb_max          REAL,
    current_ratio_min  REAL,
    eps_growth_min     REAL
);
