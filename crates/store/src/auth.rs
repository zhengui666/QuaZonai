//! Opaque local browser sessions. Local access needs no enrollment or user
//! challenge; native cookies only correlate requests. Machine authority remains
//! separate and is rechecked by the original scoped-capability transactions.
use crate::{Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{auth::BrowserSession, Id, Revision, SchemaV1};
use sqlx::{Postgres, Row, Transaction};
type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Clone)]
pub struct AuthSnapshot {
    pub initialized: bool,
    pub epoch: Revision,
    pub database_now: DateTime<Utc>,
}
#[derive(Clone, Debug)]
pub struct LoginAuthority {
    pub id: Id,
    pub epoch: Revision,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub database_now: DateTime<Utc>,
}
impl LoginAuthority {
    pub fn public(&self) -> BrowserSession {
        BrowserSession {
            schema_version: SchemaV1,
            authenticated_at: self.authenticated_at,
            expires_at: self.expires_at,
        }
    }
}
impl Store {
    pub async fn authentication_snapshot(&self) -> Result<AuthSnapshot, StoreError> {
        let row = sqlx::query("SELECT initialized,session_epoch,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton")
            .fetch_one(&self.pool).await?;
        Ok(AuthSnapshot {
            initialized: row.try_get("initialized")?,
            epoch: crate::db::revision(row.try_get("session_epoch")?)?,
            database_now: row.try_get("now")?,
        })
    }

    /// Trusted loopback HTTP adapter only. A machine request must never call this
    /// as a fallback after a missing, malformed, expired or revoked capability.
    pub async fn local_browser(&self) -> Result<LoginAuthority, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT session_epoch,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton AND initialized FOR SHARE")
            .fetch_one(&mut *tx).await?;
        let now: DateTime<Utc> = row.try_get("now")?;
        let epoch: i64 = row.try_get("session_epoch")?;
        let expires = now + Duration::hours(12);
        let row = sqlx::query("INSERT INTO app.browser_logins(auth_epoch,authenticated_at,expires_at,device_id) VALUES($1,$2,$3,NULL) RETURNING id,auth_epoch,authenticated_at,expires_at,$2::timestamptz AS now")
            .bind(epoch).bind(now).bind(expires).fetch_one(&mut *tx).await?;
        let result = authority(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn browser_authority(&self, login_id: Id) -> Result<LoginAuthority, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT singleton FROM app.operator_auth_state WHERE singleton FOR SHARE")
            .fetch_one(&mut *tx)
            .await?;
        let result = lock_login(&mut tx, login_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn logout_browser(&self, login_id: Id) -> Result<(), StoreError> {
        sqlx::query("UPDATE app.browser_logins SET revoked_at=clock_timestamp() WHERE id=$1 AND revoked_at IS NULL")
            .bind(login_id.as_uuid()).execute(&self.pool).await?;
        Ok(())
    }
}
fn authority(row: &sqlx::postgres::PgRow) -> Result<LoginAuthority, StoreError> {
    Ok(LoginAuthority {
        id: crate::db::id(row.try_get("id")?)?,
        epoch: crate::db::revision(row.try_get("auth_epoch")?)?,
        authenticated_at: row.try_get("authenticated_at")?,
        expires_at: row.try_get("expires_at")?,
        database_now: row.try_get("now")?,
    })
}
pub(crate) async fn lock_login(
    tx: &mut Tx<'_>,
    login_id: Id,
) -> Result<LoginAuthority, StoreError> {
    let row = sqlx::query("SELECT l.id,l.auth_epoch,l.authenticated_at,l.expires_at,clock_timestamp() AS now FROM app.browser_logins l JOIN app.operator_auth_state a ON a.singleton WHERE l.id=$1 AND a.initialized AND l.auth_epoch=a.session_epoch AND l.revoked_at IS NULL AND l.expires_at>clock_timestamp() AND l.device_id IS NULL FOR UPDATE OF l")
        .bind(login_id.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::AuthenticationRequired)?;
    let result = authority(&row)?;
    // Time is checked after any lock wait, immediately before authority is used.
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if result.expires_at <= now {
        return Err(StoreError::AuthenticationRequired);
    }
    Ok(LoginAuthority {
        database_now: now,
        ..result
    })
}
