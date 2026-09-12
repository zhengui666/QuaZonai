-- One immutable, relational selection before the original Mission queue ACK.
CREATE TABLE app.cycle_selections (
 cycle_id app.identity PRIMARY KEY REFERENCES app.research_cycles,
 project_id app.identity NOT NULL REFERENCES app.projects,
 research_run_id app.identity NOT NULL UNIQUE REFERENCES app.run_missions(run_id),
 policy_id app.identity NOT NULL REFERENCES app.evaluation_policies,
 created_at app.instant NOT NULL DEFAULT clock_timestamp(),
 status text NOT NULL CHECK(status IN ('COMPLETE','INCONCLUSIVE')),
 trial_count app.counter NOT NULL,
 eligible_count app.counter NOT NULL,
 selected_count app.counter NOT NULL,
 unfinished_count app.counter NOT NULL,
 CHECK(selected_count<=eligible_count AND eligible_count<=trial_count AND unfinished_count<=trial_count),
 FOREIGN KEY(cycle_id,project_id) REFERENCES app.research_cycles(id,project_id),
 FOREIGN KEY(research_run_id,project_id,cycle_id) REFERENCES app.runs(id,project_id,cycle_id),
 FOREIGN KEY(policy_id,project_id) REFERENCES app.evaluation_policies(id,project_id)
);
CREATE TABLE app.cycle_selection_trials (
 cycle_id app.identity NOT NULL REFERENCES app.cycle_selections(cycle_id) DEFERRABLE INITIALLY DEFERRED,
 experiment_id app.identity NOT NULL REFERENCES app.experiments,
 source_cycle_id app.identity NOT NULL REFERENCES app.research_cycles,
 execution_run_id app.identity REFERENCES app.runs,
 compile_run_id app.identity REFERENCES app.run_native_tasks(run_id),
 discovery_run_id app.identity REFERENCES app.run_native_tasks(run_id),
 validation_run_id app.identity REFERENCES app.run_native_tasks(run_id),
 alpha_version_id app.identity REFERENCES app.alpha_versions,
 evaluation_id app.identity REFERENCES app.evaluations,
 selection_metric_id app.identity REFERENCES app.metric_values,
 execution_state text CHECK(execution_state IN ('QUEUED','DISPATCHING','RUNNING','RECONCILING','CANCEL_REQUESTED','SUCCEEDED','FAILED','CANCELLED')),
 reason text NOT NULL CHECK(reason IN ('ELIGIBLE','INCOMPARABLE_INPUT','UNFINISHED','NOT_EXECUTED','EXECUTION_FAILED','EXECUTION_CANCELLED','NO_FORMAL_EVALUATION','INVALID_EVIDENCE','REQUIRED_METRIC_MISSING','SELECTION_METRIC_MISSING')),
 rank app.counter CHECK(rank>0),
 selected boolean NOT NULL,
 unfinished boolean NOT NULL,
 PRIMARY KEY(cycle_id,experiment_id),
 UNIQUE(cycle_id,rank),
 CHECK((reason='ELIGIBLE')=(rank IS NOT NULL)),
 CHECK(NOT selected OR rank IS NOT NULL),
 CHECK(unfinished=(reason='UNFINISHED')),
 CHECK(rank IS NULL OR (selection_metric_id IS NOT NULL AND evaluation_id IS NOT NULL))
);
CREATE FUNCTION app.guard_selection_trial() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM app.research_cycles WHERE id=NEW.cycle_id FOR UPDATE;
 IF EXISTS(SELECT 1 FROM app.cycle_selections WHERE cycle_id=NEW.cycle_id) THEN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='selection membership is sealed';
 END IF;
 IF NOT EXISTS(SELECT 1 FROM app.experiments e
   JOIN app.research_cycles c ON c.id=NEW.cycle_id AND c.project_id=e.project_id
   JOIN app.research_briefs b ON b.id=c.brief_id
   JOIN app.evaluation_policies p ON p.id=b.evaluation_policy_id AND p.family_id=e.family_id
   LEFT JOIN app.experiment_compilations compiled ON compiled.experiment_id=e.id
   LEFT JOIN app.experiment_forecasts forecast ON forecast.experiment_id=e.id
   LEFT JOIN app.experiment_validations validated ON validated.experiment_id=e.id
   LEFT JOIN app.command_receipts created ON created.principal_scope='MISSION:'||compiled.mission_run_id::text
     AND created.operation='RESEARCH_ALPHA_CREATE' AND created.idempotency_key=e.id::text
   LEFT JOIN app.alpha_versions av ON av.id=created.resource_id AND av.experiment_id=e.id
   LEFT JOIN app.runs ran ON ran.id=coalesce(validated.run_id,forecast.run_id,compiled.compile_run_id,e.run_id)
   WHERE e.id=NEW.experiment_id AND e.cycle_id=NEW.source_cycle_id
     AND NEW.compile_run_id IS NOT DISTINCT FROM compiled.compile_run_id
     AND NEW.discovery_run_id IS NOT DISTINCT FROM forecast.run_id
     AND NEW.validation_run_id IS NOT DISTINCT FROM validated.run_id
     AND NEW.alpha_version_id IS NOT DISTINCT FROM av.id
     AND NEW.execution_run_id IS NOT DISTINCT FROM ran.id
     AND NEW.execution_state IS NOT DISTINCT FROM ran.state)
   OR (NEW.evaluation_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.evaluations e
     JOIN app.evaluation_publications p ON p.evaluation_id=e.id
     WHERE e.id=NEW.evaluation_id AND e.run_id=NEW.validation_run_id
       AND e.subject_alpha_version_id=NEW.alpha_version_id AND e.evaluation_kind='WALK_FORWARD'))
   OR (NEW.selection_metric_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.metric_values
     WHERE id=NEW.selection_metric_id AND evaluation_id=NEW.evaluation_id)) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='selection retains original trial and metric identities';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER selection_member BEFORE INSERT ON app.cycle_selection_trials
 FOR EACH ROW EXECUTE FUNCTION app.guard_selection_trial();
CREATE FUNCTION app.guard_selection_header() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE n bigint; n_eligible bigint; n_selected bigint; n_unfinished bigint; requested integer; registered bigint;
BEGIN
 PERFORM id FROM app.research_cycles WHERE id=NEW.cycle_id FOR UPDATE;
 SELECT (p.selection_rule->>'candidate_count')::integer INTO requested
 FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN'
 JOIN app.evaluation_policies p ON p.id=b.evaluation_policy_id AND p.id=NEW.policy_id
 JOIN app.run_missions m ON m.cycle_id=c.id AND m.run_id=NEW.research_run_id AND m.role='RESEARCHER'
 JOIN app.runs r ON r.id=m.run_id
 JOIN app.run_terminal_receipts t ON t.run_id=r.id AND t.terminal_state=r.state
   AND t.attempt_id IS NOT DISTINCT FROM r.active_attempt_id
 WHERE c.id=NEW.cycle_id AND c.project_id=NEW.project_id;
 SELECT count(*),count(t.rank),count(*) FILTER(WHERE t.selected),count(*) FILTER(WHERE t.unfinished)
 INTO n,n_eligible,n_selected,n_unfinished FROM app.cycle_selection_trials t WHERE t.cycle_id=NEW.cycle_id;
 SELECT count(*) INTO registered FROM app.experiments e JOIN app.evaluation_policies p
 ON p.family_id=e.family_id AND p.project_id=e.project_id WHERE p.id=NEW.policy_id;
 IF requested IS NULL OR ROW(NEW.trial_count,NEW.eligible_count,NEW.selected_count,NEW.unfinished_count)
     IS DISTINCT FROM ROW(n,n_eligible,n_selected,n_unfinished)
   OR n<>registered OR n_selected<>least(n_eligible,requested)
   OR NEW.status<>(CASE WHEN n_selected=requested AND n_unfinished=0 THEN 'COMPLETE' ELSE 'INCONCLUSIVE' END)
   OR EXISTS(SELECT 1 FROM app.cycle_selection_trials WHERE cycle_id=NEW.cycle_id
     AND selected IS DISTINCT FROM coalesce(rank<=requested,false)) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='selection requires original terminal Mission and exact member counts';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER selection_header BEFORE INSERT ON app.cycle_selections
 FOR EACH ROW EXECUTE FUNCTION app.guard_selection_header();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.cycle_selections
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON app.cycle_selection_trials
 FOR EACH ROW EXECUTE FUNCTION app.reject_change();
CREATE FUNCTION app.guard_selected_cycle_proposal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM app.research_cycles WHERE id=NEW.cycle_id FOR UPDATE;
 IF EXISTS(SELECT 1 FROM app.cycle_selections WHERE cycle_id=NEW.cycle_id) THEN
  RAISE EXCEPTION USING ERRCODE='23000', MESSAGE='selected cycle cannot register new trials';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER selected_cycle BEFORE INSERT ON app.experiments
 FOR EACH ROW EXECUTE FUNCTION app.guard_selected_cycle_proposal();
