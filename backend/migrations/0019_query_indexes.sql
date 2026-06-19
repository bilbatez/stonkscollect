-- Screener ratio filters do a correlated "latest annual value per metric"
-- lookup: WHERE metric=? AND period_type='annual' AND period_end=(SELECT MAX
-- period_end for the same company/metric/period_type). The existing
-- idx_ratios_company_period leads with company_id, so a metric-first filter
-- across the ~10k-company universe can't use it. Lead with metric+period_type
-- so both the MAX(period_end) subquery and the outer filter are index-served.
CREATE INDEX idx_ratios_metric_lookup ON ratios (metric, period_type, company_id, period_end);

-- The company directory hides index pseudo-companies and delisted names by
-- default: WHERE is_index = 0 AND status = 'active'. A composite index matches
-- that predicate directly.
CREATE INDEX idx_companies_active ON companies (is_index, status);
