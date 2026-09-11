-- Derive relational identity from existing immutable selection facts. A bad
-- historical pair fails this entire migration; never rewrite its evidence.
-- Acquire write-conflicting locks before validating/reifying any old row.
LOCK TABLE app.evaluation_policies, app.experiment_families IN SHARE ROW EXCLUSIVE MODE;

ALTER TABLE app.evaluation_policies
  ADD COLUMN family_id app.identity GENERATED ALWAYS AS ((selection_rule->>'family_id')::uuid) STORED NOT NULL,
  ADD COLUMN root_lineage_id app.identity GENERATED ALWAYS AS ((selection_rule->>'root_lineage_id')::uuid) STORED NOT NULL,
  ADD CONSTRAINT policy_family_tuple UNIQUE (id,project_id,family_id,root_lineage_id);
ALTER TABLE app.experiment_families
  ADD CONSTRAINT family_policy_tuple UNIQUE (id,project_id,selection_policy_id,root_lineage_id);

ALTER TABLE app.evaluation_policies
  ADD CONSTRAINT policy_exact_family FOREIGN KEY (family_id,project_id,id,root_lineage_id)
  REFERENCES app.experiment_families(id,project_id,selection_policy_id,root_lineage_id)
  DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE app.experiment_families
  DROP CONSTRAINT experiment_families_selection_policy_id_project_id_fkey,
  ADD CONSTRAINT family_exact_policy FOREIGN KEY (selection_policy_id,project_id,id,root_lineage_id)
  REFERENCES app.evaluation_policies(id,project_id,family_id,root_lineage_id)
  DEFERRABLE INITIALLY DEFERRED;
