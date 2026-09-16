-- Preserve all original requests and source references, without rewriting history.
ALTER TABLE app.portfolio_build_tasks ALTER COLUMN snapshot_id DROP NOT NULL;
ALTER TABLE app.portfolio_build_tasks ADD COLUMN last_target_candidate_id app.identity REFERENCES app.portfolio_candidates;
ALTER TABLE app.portfolio_build_tasks ADD CONSTRAINT one_original_weight_source
 CHECK ((snapshot_id IS NOT NULL)::integer + (last_target_candidate_id IS NOT NULL)::integer = 1);
