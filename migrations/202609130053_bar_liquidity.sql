-- Existing observations and policies are never backfilled or requalified.
ALTER TABLE app.execution_assumption_sources ADD COLUMN bar_liquidity app.document;
ALTER TABLE app.execution_assumption_sources ADD COLUMN bar_liquidity_valid_until app.instant;
ALTER TABLE app.execution_assumption_sources ADD CONSTRAINT bar_liquidity_expiry_pair
 CHECK ((bar_liquidity IS NULL) = (bar_liquidity_valid_until IS NULL));
