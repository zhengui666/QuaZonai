-- Original plan belongs to its immutable policy, never to a mutable Run request.
ALTER TABLE app.evaluation_policies
 ADD COLUMN portfolio_study_plan jsonb
 CHECK (portfolio_study_plan IS NULL OR
   (jsonb_typeof(portfolio_study_plan)='object' AND portfolio_metric_requirements IS NOT NULL)),
 ADD COLUMN portfolio_study_input_set_id uuid GENERATED ALWAYS AS
   ((portfolio_study_plan->>'input_set_id')::uuid) STORED,
 ADD FOREIGN KEY (portfolio_study_input_set_id,project_id)
 REFERENCES app.input_sets(id,project_id),
 ADD CHECK (portfolio_study_plan IS NULL OR portfolio_study_input_set_id IS NOT NULL);
