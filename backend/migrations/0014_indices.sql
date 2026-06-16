-- Market index pseudo-companies (e.g. ^GSPC, ^IXIC, ^DJI) reuse the companies +
-- prices tables but are flagged so they stay out of the company directory,
-- market movers, and the screener. They are still collected like equities.
ALTER TABLE companies ADD COLUMN is_index INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_companies_is_index ON companies(is_index);
