-- Preserve immutable historical policies: missing Sealed intent is not inferred.
ALTER TABLE app.evaluation_policies
  ADD COLUMN sealed_metric_requirements jsonb,
  ADD CONSTRAINT sealed_metric_requirements_shape CHECK (
    sealed_metric_requirements IS NULL OR
    (jsonb_typeof(sealed_metric_requirements) = 'array' AND
     jsonb_array_length(sealed_metric_requirements) BETWEEN 1 AND 64)
  );
