//! Append native observations and their cursor in one transaction. No account ledger.
use crate::{authority::Actor, db, Store, StoreError};
use contracts::{
    account_observation::*,
    control::{ListQuery, Page},
    DbCounter, Id,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

fn counter(value: i64) -> Result<DbCounter, StoreError> {
    u64::try_from(value)
        .ok()
        .and_then(|v| DbCounter::new(v).ok())
        .ok_or(StoreError::Integrity)
}
fn observation(row: &PgRow) -> Result<AccountObservationV1, StoreError> {
    Ok(AccountObservationV1 {
        id: db::id(row.try_get("id")?)?,
        source_id: db::id(row.try_get("source_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        observation: serde_json::from_value(row.try_get("content")?)
            .map_err(|_| StoreError::Integrity)?,
        gap_before: row.try_get("gap_before")?,
        received_at: row.try_get("received_at")?,
    })
}
fn source(row: &PgRow) -> Result<AccountSourceV1, StoreError> {
    let now: chrono::DateTime<chrono::Utc> = row.try_get("checked_at")?;
    let observed = counter(row.try_get("last_observed_at_ns")?)?;
    let now_ns = now
        .timestamp_nanos_opt()
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(StoreError::Integrity)?;
    Ok(AccountSourceV1 {
        id: db::id(row.try_get("id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        binding: serde_json::from_value(row.try_get("binding")?)
            .map_err(|_| StoreError::Integrity)?,
        last_sequence: counter(row.try_get("last_sequence")?)?,
        dropped_events: counter(row.try_get("dropped_events")?)?,
        has_gap: row.try_get("has_gap")?,
        last_observation_id: db::id(row.try_get("last_observation_id")?)?,
        latest_snapshot_id: db::optional_id(row, "latest_snapshot_id")?,
        connection: domain::account_observation::connection(
            db::enum_value(row, "connection")?,
            observed.get(),
            now_ns,
        ),
        last_observed_at_ns: observed,
        last_received_at: row.try_get("last_received_at")?,
        checked_at: now,
    })
}

async fn authorize_read(tx: &mut Tx<'_>, actor: &Actor, project: Id) -> Result<(), StoreError> {
    crate::evidence::authorize(tx, actor, project).await?;
    sqlx::query("SELECT id FROM app.projects WHERE id=$1")
        .bind(project.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    Ok(())
}
async fn scoped_source(
    tx: &mut Tx<'_>,
    project: Id,
    id: Id,
) -> Result<AccountSourceV1, StoreError> {
    let row = sqlx::query("SELECT s.*,c.*,o.received_at AS last_received_at,clock_timestamp() AS checked_at FROM app.native_account_sources s JOIN app.native_account_cursors c ON c.source_id=s.id JOIN app.native_account_observations o ON o.id=c.last_observation_id WHERE s.project_id=$1 AND s.id=$2")
        .bind(project.as_uuid()).bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    source(&row)
}

impl Store {
    pub async fn submit_account_observation(
        &self,
        actor: &Actor,
        request: &AccountObservationSubmitV1,
    ) -> Result<AccountObservationReceiptV1, StoreError> {
        domain::account_observation::observation(request)?;
        let mut tx = self.pool.begin().await?;
        let b = &request.binding;
        // Existing Forward authorization locks the project, principal and integration.
        let downstream =
            crate::forward::source_authority(&mut tx, actor, b.project_id, b.environment).await?;
        let environment = db::code(&b.environment)?;
        let source_id = Id::new();
        sqlx::query("INSERT INTO app.native_account_sources(id,project_id,downstream_id,environment,native_trader_id,native_session_id,native_account_id,binding) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(project_id,downstream_id,environment,native_trader_id,native_session_id,native_account_id) DO NOTHING")
            .bind(source_id.as_uuid()).bind(b.project_id.as_uuid()).bind(downstream.as_uuid()).bind(&environment).bind(&b.native_trader_id).bind(&b.native_session_id).bind(&b.native_account_id).bind(db::json(b)?).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT id,binding FROM app.native_account_sources WHERE project_id=$1 AND downstream_id=$2 AND environment=$3 AND native_trader_id=$4 AND native_session_id=$5 AND native_account_id=$6")
            .bind(b.project_id.as_uuid()).bind(downstream.as_uuid()).bind(environment).bind(&b.native_trader_id).bind(&b.native_session_id).bind(&b.native_account_id).fetch_one(&mut *tx).await?;
        let source_id = db::id(row.try_get("id")?)?;
        if row.try_get::<serde_json::Value, _>("binding")? != db::json(b)? {
            return Err(StoreError::NativeIdentityConflict);
        }
        let event = request.snapshot.as_ref().map(|s| s.event_id.as_str());
        // The original complete envelope is the retry identity, including transport clocks.
        let replay = sqlx::query("SELECT o.*,s.downstream_id FROM app.native_account_observations o JOIN app.native_account_sources s ON s.id=o.source_id WHERE source_id=$1 AND (sequence=$2 OR native_event_id=$3)")
            .bind(source_id.as_uuid()).bind(request.sequence.get() as i64).bind(event).fetch_all(&mut *tx).await?;
        if !replay.is_empty() {
            if replay.len() != 1
                || replay[0].try_get::<serde_json::Value, _>("content")? != db::json(request)?
            {
                return Err(StoreError::NativeIdentityConflict);
            }
            let resource = observation(&replay[0])?;
            tx.commit().await?;
            return Ok(AccountObservationReceiptV1 {
                replayed: true,
                resource,
            });
        }
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if request.observed_at_ns.get()
            > now
                .timestamp_nanos_opt()
                .and_then(|n| u64::try_from(n).ok())
                .ok_or(StoreError::Integrity)?
        {
            return Err(StoreError::Invalid("native_account_future_time"));
        }
        let previous =
            sqlx::query("SELECT * FROM app.native_account_cursors WHERE source_id=$1 FOR UPDATE")
                .bind(source_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?;
        let (last, dropped, had_gap, latest, latest_ns) = if let Some(row) = &previous {
            let last: i64 = row.try_get("last_sequence")?;
            let dropped: i64 = row.try_get("dropped_events")?;
            if request.sequence.get() <= last as u64
                || request.dropped_events.get() < dropped as u64
                || request.observed_at_ns.get()
                    < row.try_get::<i64, _>("last_observed_at_ns")? as u64
            {
                return Err(StoreError::Invalid("native_account_cursor_regression"));
            }
            (
                last,
                dropped,
                row.try_get::<bool, _>("has_gap")?,
                db::optional_id(row, "latest_snapshot_id")?,
                row.try_get::<Option<i64>, _>("latest_snapshot_ns")?,
            )
        } else {
            (0, 0, false, None, None)
        };
        if request.dropped_events.get() - dropped as u64 > request.sequence.get() - last as u64 - 1
        {
            return Err(StoreError::Invalid("native_account_drop_count"));
        }
        let gap = request.sequence.get() != last as u64 + 1
            || request.dropped_events.get() > dropped as u64;
        let id = Id::new();
        let received_at = sqlx::query_scalar("INSERT INTO app.native_account_observations(id,source_id,sequence,native_event_id,content,gap_before) VALUES($1,$2,$3,$4,$5,$6) RETURNING received_at")
            .bind(id.as_uuid()).bind(source_id.as_uuid()).bind(request.sequence.get() as i64).bind(event).bind(db::json(request)?).bind(gap).fetch_one(&mut *tx).await?;
        let (latest, latest_ns) = match &request.snapshot {
            Some(snapshot) if latest_ns.is_none_or(|n| snapshot.ts_init.get() >= n as u64) => {
                (Some(id), Some(snapshot.ts_init.get() as i64))
            }
            _ => (latest, latest_ns),
        };
        sqlx::query("INSERT INTO app.native_account_cursors(source_id,last_sequence,dropped_events,has_gap,last_observation_id,latest_snapshot_id,last_observed_at_ns,latest_snapshot_ns,connection) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(source_id) DO UPDATE SET last_sequence=EXCLUDED.last_sequence,dropped_events=EXCLUDED.dropped_events,has_gap=EXCLUDED.has_gap,last_observation_id=EXCLUDED.last_observation_id,latest_snapshot_id=EXCLUDED.latest_snapshot_id,last_observed_at_ns=EXCLUDED.last_observed_at_ns,latest_snapshot_ns=EXCLUDED.latest_snapshot_ns,connection=EXCLUDED.connection")
            .bind(source_id.as_uuid()).bind(request.sequence.get() as i64).bind(request.dropped_events.get() as i64).bind(had_gap || gap).bind(id.as_uuid()).bind(latest.map(Id::as_uuid)).bind(request.observed_at_ns.get() as i64).bind(latest_ns).bind(db::code(&request.connection)?).execute(&mut *tx).await?;
        if crate::forward::source_authority(&mut tx, actor, b.project_id, b.environment).await?
            != downstream
        {
            return Err(StoreError::Forbidden);
        }
        tx.commit().await?;
        Ok(AccountObservationReceiptV1 {
            replayed: false,
            resource: AccountObservationV1 {
                id,
                source_id,
                downstream_id: downstream,
                observation: request.clone(),
                gap_before: gap,
                received_at,
            },
        })
    }

    pub async fn account_sources(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<AccountSourceV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authorize_read(&mut tx, actor, project).await?;
        let rows = sqlx::query("SELECT s.*,c.*,o.received_at AS last_received_at,clock_timestamp() AS checked_at FROM app.native_account_sources s JOIN app.native_account_cursors c ON c.source_id=s.id JOIN app.native_account_observations o ON o.id=c.last_observation_id WHERE s.project_id=$1 AND ($2::uuid IS NULL OR s.id<$2) ORDER BY s.id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(source).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |s| s.id))
    }

    pub async fn account_current(
        &self,
        actor: &Actor,
        project: Id,
        id: Id,
    ) -> Result<AccountCurrentV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        authorize_read(&mut tx, actor, project).await?;
        let source = scoped_source(&mut tx, project, id).await?;
        let latest_snapshot = if let Some(snapshot) = source.latest_snapshot_id {
            let row = sqlx::query("SELECT o.*,s.downstream_id FROM app.native_account_observations o JOIN app.native_account_sources s ON s.id=o.source_id WHERE o.source_id=$1 AND o.id=$2")
                .bind(id.as_uuid()).bind(snapshot.as_uuid()).fetch_one(&mut *tx).await?;
            Some(observation(&row)?)
        } else {
            None
        };
        let valuation = domain::account_observation::valuation(
            latest_snapshot
                .as_ref()
                .and_then(|s| s.observation.snapshot.as_ref()),
        );
        tx.commit().await?;
        Ok(AccountCurrentV1 {
            source,
            valuation,
            latest_snapshot,
        })
    }

    pub async fn account_observations(
        &self,
        actor: &Actor,
        project: Id,
        source: Id,
        query: &ListQuery,
    ) -> Result<Page<AccountObservationV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authorize_read(&mut tx, actor, project).await?;
        scoped_source(&mut tx, project, source).await?;
        if let Some(cursor) = query.cursor {
            sqlx::query(
                "SELECT id FROM app.native_account_observations WHERE source_id=$1 AND id=$2",
            )
            .bind(source.as_uuid())
            .bind(cursor.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::EventCursorExpired)?;
        }
        let rows = sqlx::query("SELECT o.*,s.downstream_id FROM app.native_account_observations o JOIN app.native_account_sources s ON s.id=o.source_id WHERE source_id=$1 AND ($2::uuid IS NULL OR o.id<$2) ORDER BY o.id DESC LIMIT $3")
            .bind(source.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(observation)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |s| s.id))
    }
}
