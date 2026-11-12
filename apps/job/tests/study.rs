//! Retained calendar/PIT contracts and rejection of the retired study executor.
//! These checks never claim that a custom Wasm/Clarabel strategy still executes.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::science::*;

#[test]
fn original_calendar_preserves_utc_dst_and_early_close_without_weekday_inference() {
    let (_catalog, mut request, _) = market::study();
    market::calendar_schedule(&mut request);
    let ns = |s: &str| {
        market::count(
            s.parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
                .timestamp_nanos_opt()
                .unwrap() as u64,
        )
    };
    for dates in [
        [
            ("2025-03-07T14:30:00Z", "2025-03-07T21:00:00Z"),
            ("2025-03-10T13:30:00Z", "2025-03-10T20:00:00Z"),
        ],
        [
            ("2025-11-26T14:30:00Z", "2025-11-26T21:00:00Z"),
            ("2025-11-28T14:30:00Z", "2025-11-28T18:00:00Z"),
        ],
    ] {
        for offset in [-3600_i32, 0, 3600] {
            let seconds = i64::from(offset) * 1_000_000_000;
            request.evaluation_start_ns =
                market::count(ns(dates[0].1).get().checked_add_signed(seconds).unwrap());
            request.source_selection.event_start_ns = ns(dates[0].0);
            request.research_available_through_ns = ns(dates[0].0);
            request.source_selection.event_end_ns =
                market::count(ns(dates[1].1).get().checked_add_signed(seconds).unwrap() + 1);
            request.source_selection.decision_cutoff_ns = request.source_selection.event_end_ns;
            request.mandate.rebalance_schedule.timezone = "America/New_York".into();
            request.mandate.rebalance_schedule.session_offset_seconds = Some(offset);
            request.mandate.rebalance_schedule.target_ttl_seconds = 4 * 86400;
            let calendar = &mut request.calendar.as_mut().unwrap().calendar;
            calendar.timezone = "America/New_York".into();
            calendar.available_at_ns = request.research_available_through_ns;
            calendar.coverage_start_ns = ns(dates[0].1);
            calendar.coverage_end_ns = market::count(ns(dates[1].1).get() + 1);
            calendar.sessions = dates
                .iter()
                .map(|(open, close)| NativeCalendarSessionV1 {
                    open_ns: ns(open),
                    close_ns: ns(close),
                })
                .collect();
            assert_eq!(
                domain::execution::portfolio_study_cutoffs(&request).unwrap(),
                dates
                    .iter()
                    .map(|(_, close)| market::count(
                        ns(close).get().checked_add_signed(seconds).unwrap()
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn retained_manual_cutoffs_require_exact_coverage_and_explicit_ttl() {
    let (_catalog, request, _model) = market::study();
    let mut manual = request.clone();
    let mut cutoffs = domain::execution::portfolio_study_cutoffs(&request).unwrap();
    cutoffs[1] = market::count(cutoffs[1].get() - 60_000_000_000);
    manual.manual_cutoffs_ns = Some(cutoffs.clone());
    assert!(domain::execution::portfolio_study_cutoffs(&manual).is_err());
    manual.mandate.rebalance_schedule.kind = contracts::portfolio::RebalanceKind::Manual;
    manual.mandate.rebalance_schedule.interval_seconds = None;
    // The irregular second gap requires its own explicit, frozen longer TTL.
    assert!(domain::execution::portfolio_study_cutoffs(&manual).is_err());
    manual.mandate.rebalance_schedule.target_ttl_seconds = 86_460;
    assert_eq!(
        domain::execution::portfolio_study_cutoffs(&manual).unwrap(),
        cutoffs
    );
    for invalid in [
        None,
        Some(vec![]),
        Some(vec![cutoffs[0]]),
        Some(vec![cutoffs[0], cutoffs[0]]),
        Some(vec![cutoffs[1], cutoffs[2]]),
        Some(vec![cutoffs[0], request.source_selection.event_end_ns]),
        Some(vec![cutoffs[0]; 257]),
    ] {
        let mut changed = manual.clone();
        changed.manual_cutoffs_ns = invalid;
        assert!(domain::execution::portfolio_study_cutoffs(&changed).is_err());
    }
    let mut uncovered_end = manual.clone();
    uncovered_end.manual_cutoffs_ns.as_mut().unwrap().pop();
    assert!(domain::execution::portfolio_study_cutoffs(&uncovered_end).is_err());
}

#[test]
fn retained_calendar_rejects_future_availability_and_mismatched_source_coverage() {
    let (_catalog, mut calendar, _model) = market::study();
    market::calendar_schedule(&mut calendar);
    let cutoffs = domain::execution::portfolio_study_cutoffs(&calendar).unwrap();
    let mut changed = calendar.clone();
    changed.calendar.as_mut().unwrap().calendar.available_at_ns = changed.evaluation_start_ns;
    assert!(domain::execution::portfolio_study_cutoffs(&changed).is_ok());
    for case in 0..8 {
        let mut changed = calendar.clone();
        let data = &mut changed.calendar.as_mut().unwrap().calendar;
        match case {
            0 => data.available_at_ns = market::count(changed.evaluation_start_ns.get() + 1),
            1 => {
                data.coverage_start_ns =
                    market::count(changed.evaluation_start_ns.get() + market::INTERVAL_NS + 1)
            }
            2 => data.coverage_end_ns = market::count(data.coverage_end_ns.get() - 1),
            3 => data.sessions[1] = data.sessions[0].clone(),
            4 => data.sessions[1].open_ns = data.sessions[0].open_ns,
            5 => data.calendar_ref = "OTHER".into(),
            6 => data.timezone = "America/New_York".into(),
            _ => changed.manual_cutoffs_ns = Some(cutoffs.clone()),
        }
        assert!(
            domain::execution::portfolio_study_cutoffs(&changed).is_err(),
            "calendar case {case}"
        );
    }
}

#[test]
fn retired_study_cli_cannot_execute_or_modify_original_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original-catalog-and-objects");
    std::fs::write(&original, b"preserve original inputs").unwrap();
    let output = native::command(
        &[
            "study-portfolio".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--objects".as_ref(),
            directory.path().as_os_str(),
        ],
        &serde_json::json!({}),
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("study-portfolio"));
    assert_eq!(
        std::fs::read(&original).unwrap(),
        b"preserve original inputs"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
