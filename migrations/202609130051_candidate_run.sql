-- Fail on conflicting historical facts rather than deleting or rewriting them.
CREATE UNIQUE INDEX portfolio_candidate_original_run ON app.portfolio_candidates(run_id);
