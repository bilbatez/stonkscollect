-- Multi-user-ready stub: unused now, companies.user_id stays NULL.
CREATE TABLE users (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE
);

CREATE TABLE companies (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id  INTEGER REFERENCES users(id),
    cik      TEXT NOT NULL,
    ticker   TEXT NOT NULL UNIQUE,
    name     TEXT NOT NULL,
    exchange TEXT,
    sector   TEXT,
    industry TEXT
);

CREATE TABLE filings (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    form       TEXT NOT NULL,
    period     TEXT,
    filed_at   TEXT,
    accession  TEXT NOT NULL UNIQUE,
    url        TEXT
);

CREATE TABLE financial_facts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id  INTEGER NOT NULL REFERENCES companies(id),
    statement   TEXT NOT NULL,   -- income | balance | cashflow
    line_item   TEXT NOT NULL,
    period_type TEXT NOT NULL,   -- quarterly | annual
    period_end  TEXT NOT NULL,   -- ISO date
    value       REAL NOT NULL,
    source      TEXT NOT NULL,
    fetched_at  TEXT NOT NULL,
    UNIQUE (company_id, statement, line_item, period_type, period_end, source)
);
CREATE INDEX idx_facts_company ON financial_facts (company_id, period_end);

CREATE TABLE ratios (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id  INTEGER NOT NULL REFERENCES companies(id),
    period_end  TEXT NOT NULL,
    metric      TEXT NOT NULL,
    value       REAL NOT NULL,
    computed_at TEXT NOT NULL,
    UNIQUE (company_id, period_end, metric)
);

CREATE TABLE prices (
    company_id INTEGER NOT NULL REFERENCES companies(id),
    date       TEXT NOT NULL,   -- ISO date
    close      REAL NOT NULL,
    volume     INTEGER,
    source     TEXT NOT NULL,
    PRIMARY KEY (company_id, date, source)
);
CREATE INDEX idx_prices_company_date ON prices (company_id, date);

CREATE TABLE segments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    period_end TEXT NOT NULL,
    segment    TEXT NOT NULL,
    metric     TEXT NOT NULL,
    value      REAL NOT NULL,
    source     TEXT NOT NULL,
    UNIQUE (company_id, period_end, segment, metric, source)
);

CREATE TABLE shares_outstanding (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    as_of      TEXT NOT NULL,
    shares     REAL NOT NULL,
    source     TEXT NOT NULL,
    UNIQUE (company_id, as_of, source)
);

CREATE TABLE ownership (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    holder     TEXT NOT NULL,
    kind       TEXT NOT NULL,   -- insider | institutional
    shares     REAL NOT NULL,
    as_of      TEXT NOT NULL,
    source     TEXT NOT NULL,
    UNIQUE (company_id, holder, as_of, source)
);

CREATE TABLE guidance (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    period_end TEXT NOT NULL,
    metric     TEXT NOT NULL,
    low        REAL,
    high       REAL,
    source     TEXT NOT NULL,
    issued_at  TEXT,
    UNIQUE (company_id, period_end, metric, source)
);

CREATE TABLE news (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id   INTEGER NOT NULL REFERENCES companies(id),
    title        TEXT NOT NULL,
    description  TEXT,
    url          TEXT NOT NULL,
    source       TEXT NOT NULL,
    published_at TEXT NOT NULL,
    dedup_hash   TEXT NOT NULL UNIQUE
);
CREATE INDEX idx_news_company_published ON news (company_id, published_at);

CREATE TABLE discrepancies (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    field      TEXT NOT NULL,
    period     TEXT,
    source_a   TEXT NOT NULL,
    value_a    REAL NOT NULL,
    source_b   TEXT NOT NULL,
    value_b    REAL NOT NULL,
    pct_diff   REAL NOT NULL,
    flagged_at TEXT NOT NULL
);

CREATE TABLE collection_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT NOT NULL,
    scope       TEXT,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    status      TEXT NOT NULL,   -- running | ok | error
    error       TEXT
);
