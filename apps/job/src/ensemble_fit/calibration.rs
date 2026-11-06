//! Independent official OLS fits share a borrowed training panel. Native replay
//! and the floating-point operations within each fit remain serial.
use super::{count, Row};
use anyhow::{ensure, Result};
use contracts::{ensemble_fit::NativeEnsembleCalibrationV1, DbCounter};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

// Admission estimates, not an RSS limit: linregress also owns matrices and SVD
// scratch, and the caller already retains the original training/report panels.
const WORKING_SET_BYTES: usize = 256 * 1024 * 1024;
const ESTIMATED_BYTES_PER_ROW: usize = 256;
const THREAD_STACK_BYTES: usize = 2 * 1024 * 1024;
const MIN_OBSERVATIONS_PER_WORKER: usize = 32 * 1024;
// Higher caps are opt-in: measurements show additional OLS scratch reliably
// increases RSS, while four workers do not consistently beat two end to end.
const DEFAULT_THREADS: usize = 2;

type Fits = Vec<Vec<NativeEnsembleCalibrationV1>>;
type Group<'a> = (&'a str, Vec<&'a Row>);

fn worker_count(cores: usize, limit: usize, jobs: usize, work: usize, largest: usize) -> usize {
    if work < MIN_OBSERVATIONS_PER_WORKER {
        return 1;
    }
    let scratch = largest
        .saturating_mul(ESTIMATED_BYTES_PER_ROW)
        .saturating_add(THREAD_STACK_BYTES);
    cores
        .min(limit)
        .min(jobs)
        .min(work / MIN_OBSERVATIONS_PER_WORKER)
        .min(WORKING_SET_BYTES / scratch)
        .max(1)
}

fn configured_threads() -> Result<usize> {
    match std::env::var("QZ_CALIBRATION_THREADS") {
        Ok(value) => {
            let count = value.parse::<usize>().ok().filter(|n| *n > 0);
            count.ok_or_else(|| anyhow::anyhow!("ENSEMBLE_CALIBRATION_THREADS_INVALID"))
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_THREADS),
        Err(_) => anyhow::bail!("ENSEMBLE_CALIBRATION_THREADS_INVALID"),
    }
}

pub(super) fn calibrations(rows: &[&Row], members: usize) -> Result<Fits> {
    let mut by_instrument: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        by_instrument.entry(&row.instrument).or_default().push(row);
    }
    ensure!(!by_instrument.is_empty(), "ENSEMBLE_NO_CALIBRATION_ROWS");
    let groups: Vec<_> = by_instrument.into_iter().collect();
    let workers = worker_count(
        thread::available_parallelism().map_or(1, usize::from),
        configured_threads()?,
        members.saturating_mul(groups.len()),
        members.saturating_mul(rows.len()),
        groups.iter().map(|(_, rows)| rows.len()).max().unwrap(),
    );
    run(&groups, members, workers)
}

#[derive(Default)]
struct Scratch {
    scores: Vec<f64>,
    labels: Vec<f64>,
}

impl Scratch {
    fn fit(&mut self, group: &Group<'_>, member: usize) -> Result<NativeEnsembleCalibrationV1> {
        let (instrument, rows) = group;
        self.scores.clear();
        self.labels.clear();
        self.scores.extend(rows.iter().map(|row| row.x[member]));
        self.labels.extend(rows.iter().map(|row| row.y));
        let model = crate::validation::ScoreCalibration::fit(&self.scores, &self.labels)?;
        let [intercept, slope] = model.coefficients();
        Ok(NativeEnsembleCalibrationV1 {
            instrument_id: (*instrument).into(),
            intercept,
            slope,
            training_observations: count(rows.len())?,
            last_label_available_ns: DbCounter::new(
                rows.iter().map(|row| row.label_available).max().unwrap(),
            )
            .map_err(anyhow::Error::msg)?,
        })
    }
}

fn run(groups: &[Group<'_>], members: usize, workers: usize) -> Result<Fits> {
    if workers == 1 {
        let mut scratch = Scratch::default();
        return (0..members)
            .map(|member| {
                groups
                    .iter()
                    .map(|group| scratch.fit(group, member))
                    .collect()
            })
            .collect();
    }
    let jobs = members * groups.len();
    let next = AtomicUsize::new(0);
    let first_error = AtomicUsize::new(jobs);
    let consume = || {
        let mut scratch = Scratch::default();
        let mut output = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= first_error.load(Ordering::Relaxed) {
                break;
            }
            // IDs follow the original member-then-instrument traversal. Every
            // earlier ID has already been assigned before a later fit can fail.
            let result = scratch.fit(&groups[index % groups.len()], index / groups.len());
            if result.is_err() {
                first_error.fetch_min(index, Ordering::Relaxed);
            }
            output.push((index, result));
        }
        output
    };
    let mut ordered = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 1..workers {
            match thread::Builder::new()
                .name("qz-calibration".into())
                .stack_size(THREAD_STACK_BYTES)
                .spawn_scoped(scope, &consume)
            {
                Ok(handle) => handles.push(handle),
                // Existing workers and the calling thread consume every job.
                // OS thread pressure must not make an otherwise valid fit fail.
                Err(_) => break,
            }
        }
        let mut output = consume();
        for handle in handles {
            match handle.join() {
                Ok(values) => output.extend(values),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        output
    });
    ordered.sort_unstable_by_key(|(index, _)| *index);
    let mut fits: Fits = (0..members)
        .map(|_| Vec::with_capacity(groups.len()))
        .collect();
    for (index, fit) in ordered {
        fits[index / groups.len()].push(fit?);
    }
    Ok(fits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // Frozen pre-change traversal. This oracle intentionally keeps the repeated
    // filtering and allocations, independently of production grouping/scheduling.
    fn original(rows: &[&Row], members: usize) -> Result<Fits> {
        let instruments: BTreeSet<_> = rows.iter().map(|row| row.instrument.as_str()).collect();
        (0..members)
            .map(|member| {
                instruments
                    .iter()
                    .map(|instrument| {
                        let selected: Vec<_> = rows
                            .iter()
                            .filter(|row| row.instrument.as_str() == *instrument)
                            .collect();
                        let model = crate::validation::ScoreCalibration::fit(
                            &selected.iter().map(|row| row.x[member]).collect::<Vec<_>>(),
                            &selected.iter().map(|row| row.y).collect::<Vec<_>>(),
                        )?;
                        let [intercept, slope] = model.coefficients();
                        Ok(NativeEnsembleCalibrationV1 {
                            instrument_id: (*instrument).into(),
                            intercept,
                            slope,
                            training_observations: count(selected.len())?,
                            last_label_available_ns: DbCounter::new(
                                selected
                                    .iter()
                                    .map(|row| row.label_available)
                                    .max()
                                    .unwrap(),
                            )
                            .map_err(anyhow::Error::msg)?,
                        })
                    })
                    .collect()
            })
            .collect()
    }

    fn panel() -> Vec<Row> {
        (0..128)
            .flat_map(|event| {
                (0..9).rev().filter_map(move |asset| {
                    // Unequal group sizes and input order exercise grouping.
                    (event % (asset + 1) != 1).then(|| Row {
                        instrument: format!("asset-{asset}"),
                        event,
                        available: event + 1,
                        label_event: event + 2,
                        label_available: event + 3,
                        y: (event as f64 * 0.17).sin() + asset as f64 * 0.01,
                        x: (0..5)
                            .map(|member| (event as f64 * 0.03 + member as f64).cos())
                            .collect(),
                    })
                })
            })
            .collect()
    }

    fn grouped<'a>(rows: &[&'a Row]) -> Vec<Group<'a>> {
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for row in rows {
            groups
                .entry(row.instrument.as_str())
                .or_default()
                .push(*row);
        }
        groups.into_iter().collect()
    }

    #[test]
    fn parallel_and_serial_match_original_coefficients_and_evidence_exactly() {
        let rows = panel();
        let rows: Vec<_> = rows.iter().collect();
        let expected = serde_json::to_value(original(&rows, 5).unwrap()).unwrap();
        let groups = grouped(&rows);
        for workers in [1, 2, 4, 16] {
            for _ in 0..3 {
                assert_eq!(
                    serde_json::to_value(run(&groups, 5, workers).unwrap()).unwrap(),
                    expected
                );
            }
        }
    }

    #[test]
    fn competing_failures_return_the_original_first_error() {
        let mut rows = panel();
        for row in &mut rows {
            // Member zero's last asset wins over member one's first asset,
            // regardless of which failure a worker observes first.
            if row.instrument == "asset-8" {
                row.x[0] = 2.0;
            }
            if row.instrument == "asset-0" {
                row.x[1] = f64::NAN;
            }
        }
        let rows: Vec<_> = rows.iter().collect();
        let expected = original(&rows, 5).unwrap_err().to_string();
        assert_eq!(expected, "CALIBRATION_CONSTANT_SCORE");
        let groups = grouped(&rows);
        for _ in 0..10 {
            assert_eq!(run(&groups, 5, 8).unwrap_err().to_string(), expected);
        }
    }

    #[test]
    fn scheduling_respects_small_work_cores_user_limit_and_scratch_estimate() {
        assert_eq!(worker_count(32, 32, 64, 100, 100), 1);
        assert_eq!(worker_count(1, 32, 64, 100_000, 100), 1);
        assert_eq!(worker_count(32, 2, 64, 100_000, 100), 2);
        assert_eq!(worker_count(32, 32, 2, 100_000, 100), 2);
        assert_eq!(worker_count(32, 32, 64, 100_000, 100), 3);
        assert_eq!(worker_count(32, 32, 64, 1_000_000, 1_000_000), 1);
        assert_eq!(worker_count(32, 32, 64, usize::MAX, usize::MAX), 1);
    }
}
