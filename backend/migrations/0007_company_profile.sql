-- Company enrichment: a prose business description and official website.
-- sector/industry/exchange columns already exist (from 0001) and are populated
-- by the `enrich` pass alongside these.
ALTER TABLE companies ADD COLUMN description TEXT;
ALTER TABLE companies ADD COLUMN website TEXT;
