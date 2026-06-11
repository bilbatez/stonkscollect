-- Speed up companies_due() which filters company_state by last_collected_at.
CREATE INDEX IF NOT EXISTS idx_company_state_collected ON company_state(last_collected_at);
