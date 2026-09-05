use chrono::{Duration, TimeZone, Utc};
use contracts::{
    budget::{BudgetV1, CostEnforcement, StopRuleV1},
    codex::{ModelCapabilityV1, ReasoningEffortCapability, SavedModelSettingsV1},
    evidence::*,
    runs::{ProjectState, RunState},
    DbCounter, Id, Revision, SchemaV1,
};
use domain::{admission::*, codex::*, evidence::*, runs::*, DomainError};
use proptest::prelude::*;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 0, 0, 0).unwrap()
}
fn budget() -> BudgetV1 {
    BudgetV1 {
        schema_version: SchemaV1,
        max_experiments: 20,
        max_parallel_runs: 2,
        max_turns_per_mission: 16,
        max_repair_turns: 2,
        max_wall_seconds: 3600,
        max_cpu_seconds: count(7200),
        max_memory_mib: 4096,
        max_output_bytes: count(67108864),
        max_cycles_per_day: 3,
        min_cycle_interval_seconds: 120,
        max_tokens: None,
        max_cost_decimal: None,
        cost_currency: None,
        cost_enforcement: CostEnforcement::Unavailable,
    }
}
fn stop() -> StopRuleV1 {
    StopRuleV1 {
        schema_version: SchemaV1,
        stop_on_qualified_count: 2,
        stop_on_budget: true,
        stop_on_no_improvement_trials: Some(5),
        stop_on_invalid_data: true,
    }
}
fn empty_usage() -> BudgetUsage {
    BudgetUsage {
        reserved_experiments: 0,
        used_experiments: 0,
        reserved_cpu_seconds: DbCounter::ZERO,
        active_runs: 0,
    }
}
fn request() -> Reservation {
    Reservation {
        experiments: 3,
        cpu_seconds: count(100),
        wall_seconds: 60,
        memory_mib: 256,
        output_bytes: count(1024),
    }
}
fn lease() -> AttemptLease {
    AttemptLease {
        attempt_no: 1,
        worker_owner_id: "worker-1".into(),
        owner_epoch: Revision::INITIAL,
        lease_expires_at: time() + Duration::seconds(30),
    }
}

#[test]
fn admissions_require_active_project_and_preserve_failed_reservations() {
    let usage = empty_usage();
    for state in [
        ProjectState::Draft,
        ProjectState::Paused,
        ProjectState::Archived,
    ] {
        assert_eq!(
            reserve(state, &budget(), &stop(), usage, request()),
            Err(DomainError::AdmissionClosed)
        );
    }
    let first = reserve(ProjectState::Active, &budget(), &stop(), usage, request()).unwrap();
    assert_eq!(first.reserved_experiments, 3);
    assert_eq!(first.active_runs, 1);
    let second = reserve(ProjectState::Active, &budget(), &stop(), first, request()).unwrap();
    assert_eq!(
        reserve(ProjectState::Active, &budget(), &stop(), second, request()),
        Err(DomainError::BudgetExhausted("parallel_runs"))
    );
    assert_eq!(usage, empty_usage());
    assert_eq!(second.reserved_experiments, 6);
}

#[test]
fn every_resource_and_all_internal_optuna_trials_consume_budget() {
    let usage = BudgetUsage {
        used_experiments: 18,
        ..empty_usage()
    };
    assert_eq!(
        reserve(ProjectState::Active, &budget(), &stop(), usage, request()),
        Err(DomainError::BudgetExhausted("experiments"))
    );
    let usage = BudgetUsage {
        reserved_cpu_seconds: count(7199),
        ..empty_usage()
    };
    assert_eq!(
        reserve(ProjectState::Active, &budget(), &stop(), usage, request()),
        Err(DomainError::BudgetExhausted("cpu_seconds"))
    );
    for r in [
        Reservation {
            wall_seconds: 3601,
            ..request()
        },
        Reservation {
            memory_mib: 4097,
            ..request()
        },
        Reservation {
            output_bytes: count(67108865),
            ..request()
        },
    ] {
        assert_eq!(
            reserve(ProjectState::Active, &budget(), &stop(), empty_usage(), r),
            Err(DomainError::BudgetExhausted("job_resource_limit"))
        );
    }
    let usage = BudgetUsage {
        used_experiments: u32::MAX,
        ..empty_usage()
    };
    assert!(reserve(ProjectState::Active, &budget(), &stop(), usage, request()).is_err());
    let usage = BudgetUsage {
        reserved_cpu_seconds: count(i64::MAX as u64),
        ..empty_usage()
    };
    assert!(reserve(ProjectState::Active, &budget(), &stop(), usage, request()).is_err());
}

#[test]
fn configurable_stop_flags_do_not_disable_the_hard_budget() {
    let mut rule = stop();
    rule.stop_on_budget = false;
    rule.stop_on_invalid_data = false;
    assert!(validate_budget(&budget(), &rule).is_ok());
    let usage = BudgetUsage {
        used_experiments: 20,
        ..empty_usage()
    };
    assert!(reserve(ProjectState::Active, &budget(), &rule, usage, request()).is_err());
}

#[test]
fn estimated_cost_is_never_promoted_to_exact_enforcement() {
    let mut policy = budget();
    policy.max_cost_decimal = Some("20.5".parse().unwrap());
    policy.cost_currency = Some("USD".into());
    policy.cost_enforcement = CostEnforcement::Estimated;
    assert!(validate_budget(&policy, &stop()).is_ok());
    policy.cost_enforcement = CostEnforcement::Exact;
    assert_eq!(
        validate_budget(&policy, &stop()),
        Err(DomainError::CapabilityUnavailable("exact_cost_enforcement"))
    );
    policy.cost_enforcement = CostEnforcement::Estimated;
    policy.cost_currency = None;
    assert!(validate_budget(&policy, &stop()).is_err());
    let mut policy = budget();
    policy.max_parallel_runs = 0;
    assert!(validate_budget(&policy, &stop()).is_err());
}

proptest! {
    #[test]
    fn accepted_reservation_cannot_exceed_any_frozen_aggregate(used in 0u32..25, reserved in 0u32..25, cpu in 0u64..8000, active in 0u16..5, trials in 1u32..25) {
        let usage=BudgetUsage {used_experiments:used,reserved_experiments:reserved,reserved_cpu_seconds:count(cpu),active_runs:active};
        if let Ok(next)=reserve(ProjectState::Active,&budget(),&stop(),usage,Reservation {experiments:trials,..request()}) {
            prop_assert_eq!(next.used_experiments,used);
            prop_assert_eq!(next.reserved_experiments,reserved+trials);
            prop_assert!(next.used_experiments+next.reserved_experiments<=20);
            prop_assert!(next.reserved_cpu_seconds.get()<=7200);
            prop_assert!(next.active_runs<=2);
        }
    }
}

#[test]
fn stale_expired_or_forged_worker_cannot_accept_a_result() {
    let current = lease();
    assert!(validate_owner(&current, &current, time()).is_ok());
    assert_eq!(
        validate_owner(&current, &current, current.lease_expires_at),
        Err(DomainError::StaleAttempt)
    );
    for presented in [
        AttemptLease {
            attempt_no: 2,
            ..current.clone()
        },
        AttemptLease {
            worker_owner_id: "worker-2".into(),
            ..current.clone()
        },
        AttemptLease {
            owner_epoch: current.owner_epoch.next().unwrap(),
            ..current.clone()
        },
    ] {
        assert_eq!(
            validate_owner(&current, &presented, time()),
            Err(DomainError::StaleAttempt)
        );
    }
    let expired = AttemptLease {
        lease_expires_at: time(),
        ..current.clone()
    };
    let forged = AttemptLease {
        lease_expires_at: time() + Duration::days(365),
        ..current
    };
    assert_eq!(
        validate_owner(&expired, &forged, time()),
        Err(DomainError::StaleAttempt)
    );
}

#[test]
fn cancel_request_and_acknowledged_termination_are_different() {
    let current = lease();
    for state in [
        RunState::Dispatching,
        RunState::Running,
        RunState::Reconciling,
    ] {
        assert_eq!(request_cancel(state).unwrap(), RunState::CancelRequested);
    }
    assert_eq!(
        request_cancel(RunState::Queued).unwrap(),
        RunState::Cancelled
    );
    assert_eq!(
        accept_terminal(RunState::CancelRequested, None, &current, &current, time()),
        Err(DomainError::CancelNotConfirmed)
    );
    for outcome in [
        RemoteTerminal::Succeeded,
        RemoteTerminal::Cancelled,
        RemoteTerminal::ConfirmedAbsent,
    ] {
        assert_eq!(
            accept_terminal(
                RunState::CancelRequested,
                Some(outcome),
                &current,
                &current,
                time()
            )
            .unwrap(),
            RunState::Cancelled
        );
    }
    assert_eq!(
        reconcile(RunState::CancelRequested).unwrap(),
        RunState::CancelRequested
    );
    assert_eq!(
        confirm_running(RunState::CancelRequested).unwrap(),
        RunState::CancelRequested
    );
}

#[test]
fn both_cas_winner_orders_have_one_terminal_outcome() {
    let current = lease();
    let success = accept_terminal(
        RunState::Running,
        Some(RemoteTerminal::Succeeded),
        &current,
        &current,
        time(),
    )
    .unwrap();
    assert_eq!(success, RunState::Succeeded);
    assert_eq!(request_cancel(success), Err(DomainError::TerminalRun));
    let cancelled = accept_terminal(
        request_cancel(RunState::Running).unwrap(),
        Some(RemoteTerminal::Succeeded),
        &current,
        &current,
        time(),
    )
    .unwrap();
    assert_eq!(cancelled, RunState::Cancelled);
    assert_eq!(
        accept_terminal(
            cancelled,
            Some(RemoteTerminal::Succeeded),
            &current,
            &current,
            time()
        ),
        Err(DomainError::TerminalRun)
    );
    for terminal in [RunState::Succeeded, RunState::Failed, RunState::Cancelled] {
        assert!(reconcile(terminal).is_err());
        assert!(begin_dispatch(terminal).is_err());
        assert!(confirm_running(terminal).is_err());
    }
}

#[test]
fn event_revision_guards_refuse_stale_writes_and_bigint_overflow() {
    assert_eq!(
        next_event(Revision::INITIAL, Revision::INITIAL, count(0)).unwrap(),
        (Revision::INITIAL.next().unwrap(), count(1))
    );
    assert_eq!(
        next_event(
            Revision::INITIAL,
            Revision::INITIAL.next().unwrap(),
            count(0)
        ),
        Err(DomainError::RevisionConflict)
    );
    assert!(next_event(Revision::INITIAL, Revision::INITIAL, count(i64::MAX as u64)).is_err());
    let max = Revision::try_from(i64::MAX.to_string()).unwrap();
    assert!(next_event(max, max, count(0)).is_err());
}

fn settings() -> SavedModelSettingsV1 {
    SavedModelSettingsV1 {
        schema_version: SchemaV1,
        use_default_model_settings: false,
        saved_model: None,
        saved_reasoning_effort: Some("strong-native-effort".into()),
        saved_fast_mode: false,
    }
}
fn model() -> ModelCapabilityV1 {
    ModelCapabilityV1 {
        schema_version: SchemaV1,
        id: "catalog-entry".into(),
        model: "operator-native-model".into(),
        display_name: "Native model".into(),
        hidden: false,
        default_reasoning_effort: "ordinary-native-effort".into(),
        supported_reasoning_efforts: vec![ReasoningEffortCapability {
            reasoning_effort: "strong-native-effort".into(),
            description: "native description".into(),
        }],
        is_default: true,
        fetched_at: time(),
        profile_revision: Revision::INITIAL,
    }
}
fn catalog(models: &[ModelCapabilityV1]) -> CatalogContext<'_> {
    CatalogContext {
        models,
        complete: true,
        profile_revision: Revision::INITIAL,
        valid_after: time() - Duration::minutes(5),
        observed_effective_model: Some("operator-native-model"),
    }
}

#[test]
fn system_effort_only_keeps_model_omitted_and_uses_actual_profile_capability() {
    let models = [model()];
    let saved = settings();
    let resolved = resolve_overrides(&saved, &catalog(&models)).unwrap();
    assert_eq!(resolved.model, None);
    assert_eq!(resolved.reasoning_effort, saved.saved_reasoning_effort);
    assert_eq!(saved, settings());
    let mut unknown = catalog(&models);
    unknown.observed_effective_model = None;
    assert!(resolve_overrides(&saved, &unknown).is_err()); // is_default is not an observed profile override.
}

#[test]
fn default_mode_preserves_saved_values_without_sending_any_overrides() {
    let mut saved = settings();
    saved.use_default_model_settings = true;
    saved.saved_model = Some("model-no-longer-listed".into());
    saved.saved_fast_mode = true;
    let original = saved.clone();
    let models = [];
    let mut ctx = catalog(&models);
    ctx.complete = false;
    assert_eq!(
        resolve_overrides(&saved, &ctx).unwrap(),
        ModelOverrides::default()
    );
    assert_eq!(saved, original);
}

#[test]
fn unsupported_stale_partial_and_duplicate_model_catalogs_never_silently_fallback() {
    let saved = settings();
    let models = [model()];
    let mut context = catalog(&models);
    context.complete = false;
    assert!(resolve_overrides(&saved, &context).is_err());
    let mut context = catalog(&models);
    context.profile_revision = Revision::INITIAL.next().unwrap();
    assert!(resolve_overrides(&saved, &context).is_err());
    let mut context = catalog(&models);
    context.valid_after = time() + Duration::seconds(1);
    assert!(resolve_overrides(&saved, &context).is_err());
    let mut changed = saved.clone();
    changed.saved_reasoning_effort = Some("invented-value".into());
    assert!(resolve_overrides(&changed, &catalog(&models)).is_err());
    let duplicated = [model(), model()];
    assert!(resolve_overrides(&saved, &catalog(&duplicated)).is_err());
    let mut changed = saved;
    changed.saved_model = Some("".into());
    assert!(resolve_overrides(&changed, &catalog(&models)).is_err());
}

fn requirement() -> MetricRequirementV1 {
    MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "risk".into(),
        scope: "total".into(),
        comparator: Comparator::Le,
        threshold_low: None,
        threshold_high: Some("0.2".parse().unwrap()),
        required: true,
        minimum_observations: count(30),
        method_allowlist: vec!["upstream-risk".into()],
    }
}
fn metric(evaluation_id: Id) -> MetricValueV1 {
    MetricValueV1 {
        schema_version: SchemaV1,
        evaluation_id,
        metric_code: "risk".into(),
        scope: "total".into(),
        value: Some(0.1),
        status: MetricStatus::Ok,
        reason_code: None,
        unit: "fraction".into(),
        period_start: time() - Duration::days(31),
        period_end: time(),
        observation_count: count(30),
        frequency: "daily".into(),
        annualization_factor: Some(252.0),
        method_id: "upstream-risk".into(),
        method_version: "1.0".into(),
        source_artifact_id: Id::new(),
        higher_is_better: Some(false),
    }
}
fn capability() -> MetricCapability {
    MetricCapability {
        metric_code: "risk".into(),
        method_id: "upstream-risk".into(),
        method_version: "1.0".into(),
        unit: "fraction".into(),
        frequency: "daily".into(),
    }
}

#[test]
fn actual_supported_metrics_pass_or_reject_without_computing_an_estimator() {
    let id = Id::new();
    let m = metric(id);
    assert_eq!(
        evaluate_metrics(
            id,
            &[requirement()],
            std::slice::from_ref(&m),
            &[capability()]
        )
        .unwrap()
        .decision,
        Decision::Pass
    );
    let mut m = m;
    m.value = Some(0.3);
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[m], &[capability()])
            .unwrap()
            .decision,
        Decision::Reject
    );
}

#[test]
fn missing_required_evidence_dominates_both_pass_and_reject() {
    let id = Id::new();
    let mut other = requirement();
    other.metric_code = "another-required-metric".into();
    for value in [0.1, 0.3] {
        let mut m = metric(id);
        m.value = Some(value);
        let gate =
            evaluate_metrics(id, &[requirement(), other.clone()], &[m], &[capability()]).unwrap();
        assert_eq!(gate.decision, Decision::Inconclusive);
        assert_eq!(gate.evidence_status, EvidenceStatus::Incomplete);
    }
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[], &[])
            .unwrap()
            .decision,
        Decision::Inconclusive
    );
    assert!(evaluate_metrics(id, &[], &[], &[]).is_err());
}

#[test]
fn unsupported_method_version_units_frequency_or_count_never_qualify() {
    let id = Id::new();
    for m in [
        MetricValueV1 {
            method_version: "unaudited-version".into(),
            ..metric(id)
        },
        MetricValueV1 {
            unit: "percent".into(),
            ..metric(id)
        },
        MetricValueV1 {
            frequency: "hourly".into(),
            ..metric(id)
        },
        MetricValueV1 {
            observation_count: count(29),
            ..metric(id)
        },
        MetricValueV1 {
            status: MetricStatus::Unsupported,
            value: None,
            reason_code: Some("UPSTREAM_UNSUPPORTED".into()),
            ..metric(id)
        },
    ] {
        assert_eq!(
            evaluate_metrics(id, &[requirement()], &[m], &[capability()])
                .unwrap()
                .decision,
            Decision::Inconclusive
        );
    }
    assert_eq!(
        evaluate_metrics(id, &[requirement()], &[metric(id)], &[])
            .unwrap()
            .evidence_status,
        EvidenceStatus::Unsupported
    );
}

#[test]
fn corrupt_or_mixed_evidence_is_rejected_not_coerced_to_missing_or_pass() {
    let id = Id::new();
    for m in [
        MetricValueV1 {
            value: Some(f64::NAN),
            ..metric(id)
        },
        MetricValueV1 {
            value: None,
            ..metric(id)
        },
        MetricValueV1 {
            annualization_factor: Some(f64::INFINITY),
            ..metric(id)
        },
        MetricValueV1 {
            period_end: time() - Duration::days(60),
            ..metric(id)
        },
        MetricValueV1 {
            status: MetricStatus::Failed,
            value: None,
            reason_code: None,
            ..metric(id)
        },
        metric(Id::new()),
    ] {
        assert!(evaluate_metrics(id, &[requirement()], &[m], &[capability()]).is_err());
    }
    assert!(evaluate_metrics(
        id,
        &[requirement()],
        &[metric(id), metric(id)],
        &[capability()]
    )
    .is_err());
    let mut invalid = requirement();
    invalid.threshold_low = Some("1".parse().unwrap());
    assert!(evaluate_metrics(id, &[invalid], &[metric(id)], &[capability()]).is_err());
}

#[test]
fn cancellation_does_not_erase_genuine_failure() {
    let owner = lease();
    assert_eq!(
        accept_terminal(
            RunState::CancelRequested,
            Some(RemoteTerminal::Failed),
            &owner,
            &owner,
            time()
        )
        .unwrap(),
        RunState::Failed
    );
    assert_eq!(
        accept_terminal(
            RunState::Cancelled,
            Some(RemoteTerminal::Failed),
            &owner,
            &owner,
            time()
        ),
        Err(DomainError::TerminalRun)
    );
}
#[test]
fn invalid_metric_is_not_missing_evidence() {
    let id = Id::new();
    let mut m = metric(id);
    m.status = MetricStatus::InvalidInput;
    m.value = None;
    m.reason_code = Some("INVALID_DATA".into());
    let gate = evaluate_metrics(id, &[requirement()], &[m], &[capability()]).unwrap();
    assert_eq!(gate.evidence_status, EvidenceStatus::Invalid);
    assert_eq!(gate.decision, Decision::Inconclusive);
    assert!(gate.reasons.iter().any(|r| r.ends_with("INVALID_INPUT")));
}
#[test]
fn standard_tier_is_an_absent_fast_override() {
    let models = [model()];
    let mut saved = settings();
    assert_eq!(
        resolve_overrides(&saved, &catalog(&models))
            .unwrap()
            .fast_mode,
        None
    );
    saved.saved_fast_mode = true;
    assert_eq!(
        resolve_overrides(&saved, &catalog(&models))
            .unwrap()
            .fast_mode,
        Some(true)
    );
    saved.saved_reasoning_effort = None;
    saved.saved_fast_mode = false;
    assert_eq!(
        resolve_overrides(&saved, &catalog(&models))
            .unwrap()
            .fast_mode,
        None
    );
}

#[test]
fn cost_currency_uses_iso_library_membership_not_ascii_shape() {
    let mut b = budget();
    b.max_cost_decimal = Some("10".parse().unwrap());
    b.cost_enforcement = CostEnforcement::Estimated;
    for code in ["USD", "EUR", "CNY", "JPY"] {
        b.cost_currency = Some(code.into());
        assert!(validate_budget(&b, &stop()).is_ok());
    }
    for code in ["ZZZ", "QQQ", "US", "usd", "USDT", "", " USD"] {
        b.cost_currency = Some(code.into());
        assert!(validate_budget(&b, &stop()).is_err(), "accepted {code}");
    }
}
