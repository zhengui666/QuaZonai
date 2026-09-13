//! Original weights sources, not a position ledger or a new eligibility engine.
use super::*;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod relational;

pub(super) struct Resolved {
    pub content: PortfolioCurrentWeightsV1,
    pub artifact: Option<RuntimeInputV1>,
    pub origin: DataOrigin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn last_target_reads_original_file_and_rows_without_upgrading_synthetic_sources(
        pool: sqlx::PgPool,
    ) {
        // Controlled relationship metadata and real PG/files only. This is not
        // native execution, scientific qualification or production delivery proof.
        let f = relational::fixture(&pool, relational::budget()).await;
        let (mandate, _, _) = relational::portfolio(&pool, &f).await;
        let mut tx = pool.begin().await.unwrap();
        let run = relational::candidate_run(&mut tx, &f).await;
        let id = Id::new();
        let artifact = Id::new();
        let time = now(&mut tx).await.unwrap();
        let asof = time - Duration::seconds(1);
        let until = time + Duration::hours(1);
        let targets = vec![
            AllocationTargetV1 {
                instrument_id: "Z.EXAMPLE".into(),
                currency: "USD".into(),
                weight: "0.6".parse().unwrap(),
            },
            AllocationTargetV1 {
                instrument_id: "A.EXAMPLE".into(),
                currency: "USD".into(),
                weight: "0.4".parse().unwrap(),
            },
        ];
        let document = json!({"schema_version":1,"candidate_id":id,"base_currency":"USD","asof":asof,"valid_until":until,"cash_weight":"0","targets":targets});
        let bytes = serde_json::to_vec(&document).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let objects =
            integrations::artifacts::ArtifactStore::open(&directory.path().join("objects"))
                .unwrap();
        objects.put(artifact, &bytes).unwrap();
        sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,'REPORT','application/json','qz.portfolio_targets','1','LOCAL',$4,'1',$5,'EVALUATOR_ONLY','SYNTHETIC','RUNTIME','REFERENCED')")
            .bind(artifact.as_uuid()).bind(f.project.as_uuid()).bind(run.as_uuid()).bind(artifact.to_string()).bind(bytes.len() as i64).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,created_at,run_id,solver_status,evidence_status,diagnostics_artifact_id,target_artifact_id,cash_weight,current_weights_source) VALUES($1,$2,$3,$4,$5,$6,$7,'OPTIMAL','VALID',$8,$9,0,'NONE')")
            .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(f.input_set.as_uuid()).bind(asof).bind(time).bind(run.as_uuid()).bind(f.artifact.as_uuid()).bind(artifact.as_uuid()).execute(&mut *tx).await.unwrap();
        for target in &targets {
            sqlx::query("INSERT INTO app.candidate_targets(candidate_id,instrument_id,target_weight,currency,asof,valid_until) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(id.as_uuid()).bind(&target.instrument_id).bind(target.weight.as_decimal()).bind(&target.currency).bind(asof).bind(until).execute(&mut *tx).await.unwrap();
        }
        let request: PortfolioBuildRequestV1 = serde_json::from_value(json!({"schema_version":1,"cycle_id":f.cycle,"mandate_id":mandate,"input_set_id":f.input_set,"runtime_id":Id::new(),"expected_runtime_revision":"1","current_weights_source":{"kind":"LAST_TARGET","candidate_id":id},"environment":"LIVE","members":[{"qualification_id":Id::new(),"ensemble_weight":"0.5"},{"qualification_id":Id::new(),"ensemble_weight":"0.5"}],"limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":10,"memory_mib":64,"output_bytes":"1024"}})).unwrap();
        let mut read = |id, size| {
            std::future::ready(objects.read(id, size).map_err(|_| StoreError::Integrity))
        };
        assert!(matches!(
            resolve(&mut tx, f.project, &request, &mut read).await,
            Err(StoreError::Invalid("portfolio_last_target"))
        ));
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let resolved = resolve(&mut tx, f.project, &request, &mut read)
            .await
            .unwrap();
        assert!(resolved.artifact.is_none());
        assert_eq!(resolved.origin, DataOrigin::Synthetic);
        assert_eq!(
            resolved.content.source,
            PortfolioWeightsSourceV1::LastTarget { candidate_id: id }
        );
        assert_eq!(resolved.content.weights, targets);
        assert_eq!(resolved.content.asof_ns, nanos(asof).unwrap());
        assert_eq!(resolved.content.available_ns, nanos(time).unwrap());
        assert_eq!(resolved.content.valid_until_ns, nanos(until).unwrap());
        assert!(matches!(
            resolve(&mut tx, Id::new(), &request, &mut read).await,
            Err(StoreError::Invalid("portfolio_last_target"))
        ));
        let mut bad = document;
        bad["targets"][0]["weight"] = json!("0.7");
        let bytes = serde_json::to_vec(&bad).unwrap();
        let mut corrupt = |_, _| std::future::ready(Ok(bytes.clone()));
        assert!(matches!(
            resolve(&mut tx, f.project, &request, &mut corrupt).await,
            Err(StoreError::Integrity)
        ));
        tx.rollback().await.unwrap();
        let error = sqlx::query(
            "INSERT INTO app.portfolio_build_tasks(run_id,mandate_id,request) VALUES($1,$2,$3)",
        )
        .bind(f.run.as_uuid())
        .bind(mandate.as_uuid())
        .bind(json!(request))
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23514")
        );
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetDocument {
    schema_version: SchemaV1,
    candidate_id: Id,
    base_currency: String,
    asof: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    cash_weight: contracts::DecimalValue,
    targets: Vec<AllocationTargetV1>,
}

fn nanos(time: DateTime<Utc>) -> Result<DbCounter, StoreError> {
    counter(time.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
}

pub(super) async fn resolve<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioBuildRequestV1,
    read: &mut R,
) -> Result<Resolved, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    match request.current_weights_source {
        PortfolioBuildWeightsV1::ForwardSnapshot { snapshot_id } => {
            let row = sqlx::query("SELECT s.*,a.byte_count,a.storage_version FROM app.forward_weight_snapshots s JOIN app.artifacts a ON a.id=s.report_artifact_id AND a.project_id=s.project_id AND a.kind='REPORT' AND a.schema_name='qz.portfolio_current_weights' AND a.schema_version='1' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.access_class='RESEARCH' JOIN app.downstream_integrations d ON d.id=s.downstream_id AND d.enabled AND (d.environments='BOTH' OR d.environments=s.environment) WHERE s.id=$1 AND s.project_id=$2 AND s.environment=$3 FOR SHARE OF d")
                .bind(snapshot_id.as_uuid()).bind(project.as_uuid()).bind(db::code(&request.environment)?).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("portfolio_weights_source"))?;
            let content: PortfolioCurrentWeightsV1 =
                serde_json::from_value(row.try_get("content")?)
                    .map_err(|_| StoreError::Integrity)?;
            let id = db::id(row.try_get("report_artifact_id")?)?;
            let size = counter(row.try_get("byte_count")?)?;
            if size == DbCounter::ZERO || size.get() > 1024 * 1024 {
                return Err(StoreError::Integrity);
            }
            let bytes = read(id, size).await?;
            let original: PortfolioCurrentWeightsV1 =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            if bytes.len() as u64 != size.get()
                || original != content
                || content.source
                    != (PortfolioWeightsSourceV1::ForwardSnapshot {
                        downstream_id: db::id(row.try_get("downstream_id")?)?,
                        external_message_id: row.try_get("external_message_id")?,
                    })
            {
                return Err(StoreError::Integrity);
            }
            Ok(Resolved {
                content,
                artifact: Some(RuntimeInputV1::Artifact {
                    artifact_id: id,
                    storage_version: row.try_get("storage_version")?,
                    byte_count: size,
                    role: ArtifactInputRole::Report,
                }),
                origin: if request.environment == contracts::forward::ForwardEnvironmentV1::Paper {
                    DataOrigin::Synthetic
                } else {
                    DataOrigin::Real
                },
            })
        }
        PortfolioBuildWeightsV1::LastTarget { candidate_id } => {
            let row = sqlx::query("SELECT c.*,a.byte_count,a.origin,m.base_currency FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.runs r ON r.id=c.run_id AND r.project_id=c.project_id AND r.state='SUCCEEDED' JOIN app.portfolio_mandates m ON m.id=c.mandate_id JOIN app.artifacts a ON a.id=c.target_artifact_id AND a.project_id=c.project_id AND a.producer_run_id=c.run_id AND a.producer_attempt_id IS NOT DISTINCT FROM r.active_attempt_id AND a.kind='REPORT' AND a.schema_name='qz.portfolio_targets' AND a.schema_version='1' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1' AND a.access_class='EVALUATOR_ONLY' AND a.origin IN ('REAL','SYNTHETIC') WHERE c.id=$1 AND c.project_id=$2 AND c.evidence_status='VALID' AND c.solver_status IN ('OPTIMAL','ACCEPTABLE_INACCURATE') AND c.cash_weight IS NOT NULL")
                .bind(candidate_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("portfolio_last_target"))?;
            let id = db::id(row.try_get("target_artifact_id")?)?;
            let size = counter(row.try_get("byte_count")?)?;
            if size == DbCounter::ZERO || size.get() > 1024 * 1024 {
                return Err(StoreError::Integrity);
            }
            let bytes = read(id, size).await?;
            let document: TargetDocument =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            let _version = document.schema_version;
            if bytes.len() as u64 != size.get()
                || document.candidate_id != candidate_id
                || document.base_currency != row.try_get::<String, _>("base_currency")?
                || document.asof != row.try_get::<DateTime<Utc>, _>("decision_asof")?
                || *document.cash_weight.as_decimal()
                    != row.try_get::<bigdecimal::BigDecimal, _>("cash_weight")?
            {
                return Err(StoreError::Integrity);
            }
            let targets = sqlx::query(
                "SELECT * FROM app.candidate_targets WHERE candidate_id=$1 ORDER BY instrument_id",
            )
            .bind(candidate_id.as_uuid())
            .fetch_all(&mut **tx)
            .await?;
            let mut sorted = document.targets.clone();
            sorted.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
            if targets.is_empty()
                || targets.len() != sorted.len()
                || targets.len() > MAX_ALLOCATION_ASSETS
            {
                return Err(StoreError::Integrity);
            }
            for (target, original) in targets.iter().zip(&sorted) {
                if target.try_get::<String, _>("instrument_id")? != original.instrument_id
                    || target.try_get::<String, _>("currency")? != original.currency
                    || target.try_get::<bigdecimal::BigDecimal, _>("target_weight")?
                        != *original.weight.as_decimal()
                    || target.try_get::<DateTime<Utc>, _>("asof")? != document.asof
                    || target.try_get::<DateTime<Utc>, _>("valid_until")? != document.valid_until
                {
                    return Err(StoreError::Integrity);
                }
            }
            let created: DateTime<Utc> = row.try_get("created_at")?;
            let content = PortfolioCurrentWeightsV1 {
                schema_version: SchemaV1,
                source: PortfolioWeightsSourceV1::LastTarget { candidate_id },
                asof_ns: nanos(document.asof)?,
                available_ns: nanos(created.max(document.asof))?,
                valid_until_ns: nanos(document.valid_until)?,
                base_currency: document.base_currency,
                cash_weight: document.cash_weight,
                weights: document.targets,
            };
            if content.available_ns >= content.valid_until_ns {
                return Err(StoreError::Invalid("portfolio_last_target_expired"));
            }
            Ok(Resolved {
                content,
                artifact: None,
                origin: if request.environment == contracts::forward::ForwardEnvironmentV1::Paper {
                    DataOrigin::Synthetic
                } else {
                    db::enum_value(&row, "origin")?
                },
            })
        }
    }
}
