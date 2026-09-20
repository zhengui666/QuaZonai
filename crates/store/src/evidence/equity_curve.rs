//! Read-only, bounded access to one already published native Study projection.
use super::{count, read_evaluation};
use crate::{authority::Actor, db, Store, StoreError};
use contracts::{
    equity_curve::{EquityCurveDataV1, EquityCurveQuery, EquityCurveV1, EquityUnavailableReason},
    evidence::{EvaluationKind, EvidenceStatus},
    runtime_jobs::{RuntimeResultState, MAX_JOB_OUTPUT_BYTES},
    science::NativePortfolioStudyResultV1,
    DbCounter, Id, SchemaV1,
};
use serde::Deserialize;
use sqlx::Row;

#[derive(Deserialize)]
struct Publication {
    schema_version: SchemaV1,
    evaluation_id: Id,
    candidate_id: Id,
    run_id: Id,
    input_set_id: Id,
    evaluation_kind: EvaluationKind,
    native_reports: Vec<(String, Id)>,
}

impl Store {
    pub async fn evaluation_equity_curve<R, Read>(
        &self,
        actor: &Actor,
        id: Id,
        query: &EquityCurveQuery,
        mut read: R,
    ) -> Result<EquityCurveV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        domain::execution::equity_curve_query(query)?;
        let mut tx = self.pool.begin().await?;
        let evaluation = read_evaluation(&mut tx, actor, id).await?;
        if evaluation.evaluation_kind != EvaluationKind::Portfolio {
            return Err(StoreError::NotFound);
        }
        let mut view = EquityCurveV1 {
            schema_version: SchemaV1,
            project_id: evaluation.project_id,
            candidate_id: evaluation
                .subject_candidate_id
                .ok_or(StoreError::Integrity)?,
            evaluation_id: evaluation.id,
            run_id: evaluation.run_id,
            origin: evaluation.origin,
            curve: EquityCurveDataV1::Unavailable {
                reason_code: EquityUnavailableReason::NoSimulation,
            },
        };
        if evaluation.execution_status != RuntimeResultState::Succeeded {
            view.curve = EquityCurveDataV1::Unavailable {
                reason_code: EquityUnavailableReason::SimulationFailed,
            };
            tx.commit().await?;
            return Ok(view);
        }
        if matches!(
            evaluation.evidence_status,
            EvidenceStatus::Invalid | EvidenceStatus::Unsupported
        ) {
            view.curve = EquityCurveDataV1::Unavailable {
                reason_code: EquityUnavailableReason::InvalidEvidence,
            };
            tx.commit().await?;
            return Ok(view);
        }
        // read_evaluation already binds the publication to this project's terminal
        // Run and frozen PORTFOLIO input. Neither eligibility nor PASS is required.
        let report_size: i64 =
            sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
                .bind(evaluation.report_artifact_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let report_size = count(report_size)?;
        if report_size.get() == 0 || report_size.get() > 8 * 1024 * 1024 {
            return Err(StoreError::Integrity);
        }
        let sources = sqlx::query("SELECT a.id,a.byte_count FROM app.runs r JOIN app.run_attempts t ON t.id=r.active_attempt_id AND t.run_id=r.id AND t.dispatch_state='TERMINAL' AND t.accepted_at IS NOT NULL JOIN app.run_native_outputs o ON o.attempt_id=t.id JOIN app.artifacts a ON a.id=o.artifact_id AND a.producer_run_id=r.id AND a.producer_attempt_id=t.id AND a.project_id=r.project_id WHERE r.id=$1 AND r.project_id=$2 AND a.schema_name='qz.portfolio_study' AND a.schema_version='1' AND a.access_class='EVALUATOR_ONLY' AND a.media_type='application/json' AND a.origin=$3")
            .bind(view.run_id.as_uuid()).bind(view.project_id.as_uuid()).bind(db::code(&view.origin)?)
            .fetch_all(&mut *tx).await?;
        if sources.len() > 1 {
            return Err(StoreError::Integrity);
        }
        let source = sources
            .first()
            .map(|row| {
                Ok::<_, StoreError>((
                    db::id(row.try_get("id")?)?,
                    count(row.try_get("byte_count")?)?,
                ))
            })
            .transpose()?;
        tx.commit().await?;
        let report = read(evaluation.report_artifact_id, report_size).await?;
        if report.len() as u64 != report_size.get() {
            return Err(StoreError::Integrity);
        }
        let expected = (
            view.evaluation_id,
            view.candidate_id,
            view.run_id,
            evaluation.input_set_id,
        );
        let referenced = tokio::task::spawn_blocking(move || {
            let document: Publication =
                serde_json::from_slice(&report).map_err(|_| StoreError::Integrity)?;
            let _schema = document.schema_version;
            if (
                document.evaluation_id,
                document.candidate_id,
                document.run_id,
                document.input_set_id,
            ) != expected
                || document.evaluation_kind != EvaluationKind::Portfolio
            {
                return Err(StoreError::Integrity);
            }
            let mut studies = document
                .native_reports
                .into_iter()
                .filter(|(name, _)| name == "qz.portfolio_study");
            let first = studies.next().map(|(_, id)| id);
            if studies.next().is_some() {
                return Err(StoreError::Integrity);
            }
            Ok(first)
        })
        .await
        .map_err(|_| StoreError::Integrity)??;
        let (source_id, source_size) = match (source, referenced) {
            (Some((id, size)), Some(reference)) if id == reference => (id, size),
            (None, None) => {
                view.curve = EquityCurveDataV1::Unavailable {
                    reason_code: EquityUnavailableReason::LegacySnapshotsUnavailable,
                };
                return Ok(view);
            }
            _ => return Err(StoreError::Integrity),
        };
        if source_size.get() == 0 || source_size.get() > MAX_JOB_OUTPUT_BYTES {
            return Err(StoreError::Integrity);
        }
        let bytes = read(source_id, source_size).await?;
        if bytes.len() as u64 != source_size.get() {
            return Err(StoreError::Integrity);
        }
        let query = query.clone();
        view.curve = tokio::task::spawn_blocking(move || {
            let result: NativePortfolioStudyResultV1 =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            match (result.simulation_request, result.simulation) {
                (None, None) => Ok(EquityCurveDataV1::Unavailable {
                    reason_code: EquityUnavailableReason::NoSimulation,
                }),
                (Some(request), Some(result)) => {
                    if result.canonical_result.get("portfolio_snapshots").is_none()
                        || result.canonical_result["portfolio_snapshots"]
                            .as_array()
                            .is_some_and(Vec::is_empty)
                    {
                        return Ok(EquityCurveDataV1::Unavailable {
                            reason_code: EquityUnavailableReason::LegacySnapshotsUnavailable,
                        });
                    }
                    let series =
                        domain::execution::portfolio_equity_curve(&request, &result, &query)?;
                    Ok(EquityCurveDataV1::Ready {
                        source_artifact_id: source_id,
                        series,
                    })
                }
                _ => Err(StoreError::Integrity),
            }
        })
        .await
        .map_err(|_| StoreError::Integrity)??;
        Ok(view)
    }
}
