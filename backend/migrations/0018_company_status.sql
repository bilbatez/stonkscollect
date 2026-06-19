-- Listing status for a company: 'active' (default) or 'delisted'. Delisted
-- companies are kept (history preserved) but hidden from the directory/movers
-- unless explicitly requested.
ALTER TABLE companies ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
