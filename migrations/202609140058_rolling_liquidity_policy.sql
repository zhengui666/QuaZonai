-- Explicit declared rolling policy, never a backfilled historical liquidity observation.
ALTER TABLE app.execution_assumption_sources ADD COLUMN rolling_liquidity app.document;
ALTER TABLE app.execution_assumption_sources ADD CONSTRAINT exclusive_liquidity_policy
 CHECK (rolling_liquidity IS NULL OR bar_liquidity IS NULL);
