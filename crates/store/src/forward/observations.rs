//! Read original history without invoking classification or wake scheduling.
use super::*;
use contracts::control::{ListQuery, Page};

impl Store {
    pub async fn forward_observations(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<ForwardObservationViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT * FROM app.degradation_observations WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(|row| {
                Ok(ForwardObservationViewV1 {
                    id: db::id(row.try_get("id")?)?,
                    project_id: db::id(row.try_get("project_id")?)?,
                    release_id: db::id(row.try_get("release_id")?)?,
                    evaluation_id: db::id(row.try_get("evaluation_id")?)?,
                    policy_id: db::id(row.try_get("policy_id")?)?,
                    classification: db::enum_value(row, "classification")?,
                    reason_codes: row.try_get("reason_codes")?,
                    observed_at: row.try_get("observed_at")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |item| item.id))
    }
    pub async fn wake_events(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<WakeViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT * FROM app.wake_events WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(|row| {
                Ok(WakeViewV1 {
                    id: db::id(row.try_get("id")?)?,
                    project_id: db::id(row.try_get("project_id")?)?,
                    observation_id: db::optional_id(row, "observation_id")?,
                    trigger: db::enum_value(row, "trigger")?,
                    state: db::enum_value(row, "state")?,
                    not_before: row.try_get("not_before")?,
                    consumed_cycle_id: db::optional_id(row, "consumed_cycle_id")?,
                    reason: row.try_get("reason")?,
                    revision: db::revision(row.try_get("revision")?)?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |item| item.id))
    }
}
