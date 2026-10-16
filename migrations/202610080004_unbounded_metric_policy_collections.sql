-- Preserve existing policies and original type/presence rules. A policy may
-- require independently checked metrics for every selected asset; its collection
-- length is not a scientific anti-overfit budget or a source identity boundary.
ALTER TABLE app.evaluation_policies
  DROP CONSTRAINT sealed_metric_requirements_shape,
  ADD CONSTRAINT sealed_metric_requirements_shape CHECK (
    sealed_metric_requirements IS NULL OR
    (jsonb_typeof(sealed_metric_requirements) = 'array' AND
     jsonb_array_length(sealed_metric_requirements) >= 1)
  );

ALTER TABLE app.evaluation_policies
  DROP CONSTRAINT evaluation_policies_portfolio_metric_requirements_check,
  ADD CONSTRAINT evaluation_policies_portfolio_metric_requirements_check CHECK (
    portfolio_metric_requirements IS NULL OR
    (jsonb_typeof(portfolio_metric_requirements) = 'array' AND
     jsonb_array_length(portfolio_metric_requirements) >= 1)
  );
