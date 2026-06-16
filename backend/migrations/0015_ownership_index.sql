-- Speed up per-company holder lookups (insider Form 4 ingestion).
CREATE INDEX idx_ownership_company ON ownership(company_id, as_of);
