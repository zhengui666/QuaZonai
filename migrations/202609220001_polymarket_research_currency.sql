-- Add native research collateral without relabeling or converting any stored amount.
-- Model cost/budget currency is deliberately unchanged. Old releases and evidence retain
-- their exact original units; this migration does not grant new scientific eligibility.
ALTER TABLE app.benchmark_versions
 ALTER COLUMN currency TYPE text USING currency::text;
ALTER TABLE app.execution_assumptions
 ALTER COLUMN base_currency TYPE text USING base_currency::text;
ALTER TABLE app.research_briefs
 ALTER COLUMN base_currency TYPE text USING base_currency::text;
ALTER TABLE app.portfolio_mandates
 ALTER COLUMN base_currency TYPE text USING base_currency::text;
ALTER TABLE app.candidate_targets
 ALTER COLUMN currency TYPE text USING currency::text;
