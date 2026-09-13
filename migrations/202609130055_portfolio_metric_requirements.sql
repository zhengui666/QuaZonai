-- No criteria are copied into historical immutable policies.
ALTER TABLE app.evaluation_policies ADD COLUMN portfolio_metric_requirements jsonb
 CHECK (portfolio_metric_requirements IS NULL OR
   (jsonb_typeof(portfolio_metric_requirements)='array'
    AND jsonb_array_length(portfolio_metric_requirements) BETWEEN 1 AND 64));
