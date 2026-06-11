# Data Models

Three representations of the same data exist: Rust structs (`domain.rs`), SQLite tables (`migrations/`), and TypeScript interfaces (`types.ts`). This page shows them side-by-side.

---

## Companies

**SQLite table: `companies`**
```sql
CREATE TABLE companies (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  cik         TEXT NOT NULL,
  ticker      TEXT NOT NULL UNIQUE,
  name        TEXT NOT NULL,
  exchange    TEXT,
  sector      TEXT,
  industry    TEXT,
  description TEXT,
  website     TEXT
);
```

**Rust: `domain::Company`**
```rust
pub struct Company {
    pub id: i64,
    pub cik: String,
    pub ticker: String,
    pub name: String,
    pub exchange: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
}
```

**TypeScript: `types.Company`**
```ts
interface Company {
  id: number
  cik: string
  ticker: string
  name: string
  exchange: string | null
  sector: string | null
  industry: string | null
  description: string | null
  website: string | null
}
```

---

## Financial Facts

**SQLite table: `financial_facts`**
```sql
CREATE TABLE financial_facts (
  company_id  INTEGER NOT NULL REFERENCES companies(id),
  statement   TEXT NOT NULL,       -- 'income' | 'balance' | 'cashflow'
  line_item   TEXT NOT NULL,       -- normalized key e.g. 'Revenue'
  period_type TEXT NOT NULL,       -- 'annual' | 'quarterly'
  period_end  TEXT NOT NULL,       -- ISO date YYYY-MM-DD
  value       REAL NOT NULL,
  source      TEXT NOT NULL,       -- 'edgar' | 'fmp' | ...
  fetched_at  TEXT NOT NULL,
  UNIQUE(company_id, statement, line_item, period_type, period_end, source)
);
```

**Rust: `domain::FinancialFact`**
```rust
pub struct FinancialFact {
    pub company_id: i64,
    pub statement: StatementKind,   // enum: Income | Balance | CashFlow
    pub line_item: String,
    pub period_type: PeriodType,    // enum: Annual | Quarterly
    pub period_end: NaiveDate,
    pub value: f64,
    pub source: String,
    pub fetched_at: DateTime<Utc>,
}
```

---

## Prices

**SQLite table: `prices`**
```sql
CREATE TABLE prices (
  company_id  INTEGER NOT NULL REFERENCES companies(id),
  date        TEXT NOT NULL,
  open        REAL,
  high        REAL,
  low         REAL,
  close       REAL,
  volume      INTEGER,
  source      TEXT NOT NULL,
  UNIQUE(company_id, date, source)
);
```

**Rust: `domain::PricePoint`**
```rust
pub struct PricePoint {
    pub company_id: i64,
    pub date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: f64,
    pub volume: Option<i64>,
    pub source: String,
}
```

---

## Ratios

**SQLite table: `ratios`**
```sql
CREATE TABLE ratios (
  company_id  INTEGER NOT NULL REFERENCES companies(id),
  period_end  TEXT NOT NULL,
  period_type TEXT NOT NULL,
  metric      TEXT NOT NULL,
  value       REAL NOT NULL,
  computed_at TEXT NOT NULL,
  UNIQUE(company_id, period_end, period_type, metric)
);
```

Metrics computed: `pe`, `pb`, `roe`, `net_margin`, `gross_margin`, `operating_margin`, `debt_to_equity`, `current_ratio`, `book_value_per_share`, `payout_ratio`, `working_capital`, `free_cash_flow`, `fcf_margin`. See [Financial Data](financials.md) for formulas.

---

## Graham Scores

**SQLite table: `graham_scores`**
```sql
CREATE TABLE graham_scores (
  company_id        INTEGER NOT NULL REFERENCES companies(id) UNIQUE,
  score             INTEGER NOT NULL,    -- 0–7 criteria passed
  passes_defensive  INTEGER NOT NULL,    -- 0 | 1
  graham_number     REAL,
  ncav_per_share    REAL,
  margin_of_safety  REAL,
  net_net           INTEGER NOT NULL,
  computed_at       TEXT NOT NULL
);
```

---

## News

**SQLite table: `news`**
```sql
CREATE TABLE news (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  company_id  INTEGER NOT NULL REFERENCES companies(id),
  title       TEXT NOT NULL,
  description TEXT,
  url         TEXT NOT NULL,
  source      TEXT NOT NULL,
  published_at TEXT,
  dedup_hash  TEXT NOT NULL UNIQUE
);
```

---

## Discrepancies

**SQLite table: `discrepancies`**
```sql
CREATE TABLE discrepancies (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  company_id  INTEGER NOT NULL REFERENCES companies(id),
  field       TEXT NOT NULL,
  period      TEXT NOT NULL,
  source_a    TEXT NOT NULL,
  value_a     REAL NOT NULL,
  source_b    TEXT NOT NULL,
  value_b     REAL NOT NULL,
  pct_diff    REAL NOT NULL,
  flagged_at  TEXT NOT NULL
);
```

---

## Users, Sessions & Watchlists

`users` starts as an email-only stub (`0001_init.sql`); `password_hash` and the
`sessions`/`watchlists` tables are added in `0003_auth.sql`.
```sql
CREATE TABLE users (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  email         TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL DEFAULT ''   -- added by 0003_auth.sql
);

CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY,             -- SHA-256 of the bearer token
  user_id    INTEGER NOT NULL REFERENCES users(id),
  expires_at TEXT NOT NULL
);

CREATE TABLE watchlists (
  user_id    INTEGER NOT NULL REFERENCES users(id),
  company_id INTEGER NOT NULL REFERENCES companies(id),
  PRIMARY KEY (user_id, company_id)
);
```

Passwords are hashed with Argon2id. Session tokens are random values; only their
SHA-256 hash is stored (the raw token never touches the DB).

---

## Collection Runs

**SQLite table: `collection_runs`** (`0001_init.sql`)
```sql
CREATE TABLE collection_runs (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  source      TEXT NOT NULL,      -- 'edgar' | 'fmp' | 'yahoo' | tier label
  scope       TEXT,               -- nullable: 'AAPL' | 'all'
  started_at  TEXT NOT NULL,
  finished_at TEXT,
  status      TEXT NOT NULL,      -- 'running' | 'ok' | 'error'
  error       TEXT
);
```

---

## Notes

**SQLite table: `notes`** (`0009_notes.sql`) — composite primary key, no surrogate id;
rows cascade-delete with their user or company.
```sql
CREATE TABLE notes (
  user_id    INTEGER  NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  company_id INTEGER  NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  body       TEXT     NOT NULL,
  updated_at DATETIME NOT NULL,
  PRIMARY KEY (user_id, company_id)
);
```
