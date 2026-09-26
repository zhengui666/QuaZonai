//! Password-verified browser admission and revocable owner CLI devices.
use crate::{authority::Actor, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    auth::{BrowserSession, CliDevice},
    Id, Revision, SchemaV1,
};
use sqlx::{Postgres, Row, Transaction};
type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Clone)]
pub struct AuthSnapshot {
    pub initialized: bool,
    pub epoch: Revision,
    pub database_now: DateTime<Utc>,
    pub password_verifier: Option<String>,
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
        let row = sqlx::query("SELECT initialized,session_epoch,password_verifier,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton")
            .fetch_one(&self.pool).await?;
        Ok(AuthSnapshot {
            initialized: row.try_get("initialized")?,
            epoch: crate::db::revision(row.try_get("session_epoch")?)?,
            database_now: row.try_get("now")?,
            password_verifier: row.try_get("password_verifier")?,
        })
    }

    /// Trusted relational callers only; HTTP admission uses password_login.
    /// Never use this as a fallback for missing browser or machine credentials.
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

    pub async fn setup_password(
        &self,
        verifier: &str,
        remember: bool,
    ) -> Result<LoginAuthority, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT password_verifier FROM app.operator_auth_state WHERE singleton FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        if row
            .try_get::<Option<String>, _>("password_verifier")?
            .is_some()
        {
            return Err(StoreError::Conflict);
        }
        sqlx::query("UPDATE app.operator_auth_state SET password_verifier=$1,session_epoch=session_epoch+1 WHERE singleton")
            .bind(verifier).execute(&mut *tx).await?;
        let login = insert_login(&mut tx, remember).await?;
        tx.commit().await?;
        Ok(login)
    }

    /// Caller verified the exact salted hash; a locked reread closes password-change races.
    pub async fn password_login(
        &self,
        snapshot: &AuthSnapshot,
        remember: bool,
    ) -> Result<LoginAuthority, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_password(&mut tx, snapshot).await?;
        let login = insert_login(&mut tx, remember).await?;
        tx.commit().await?;
        Ok(login)
    }

    pub async fn change_password(
        &self,
        login_id: Id,
        snapshot: &AuthSnapshot,
        verifier: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM app.operator_auth_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
        lock_password(&mut tx, snapshot).await?;
        lock_login(&mut tx, login_id).await?;
        sqlx::query("UPDATE app.operator_auth_state SET password_verifier=$1,session_epoch=session_epoch+1 WHERE singleton")
            .bind(verifier).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn register_cli_device(
        &self,
        snapshot: &AuthSnapshot,
        name: &str,
        verifier: &str,
    ) -> Result<CliDevice, StoreError> {
        if name.trim().is_empty()
            || name.chars().count() > 100
            || name.chars().any(char::is_control)
        {
            return Err(StoreError::Invalid("device_name"));
        }
        let mut tx = self.pool.begin().await?;
        lock_password(&mut tx, snapshot).await?;
        let row = sqlx::query("INSERT INTO app.cli_devices(name,verifier) VALUES($1,$2) RETURNING id,name,created_at,last_used_at")
            .bind(name).bind(verifier).fetch_one(&mut *tx).await?;
        let result = device(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cli_device_verifier(&self, id: Id) -> Result<String, StoreError> {
        sqlx::query_scalar(
            "SELECT verifier FROM app.cli_devices WHERE id=$1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::InvalidCredentials)
    }

    pub async fn cli_device_session(&self, actor: &Actor) -> Result<CliDevice, StoreError> {
        let mut tx = self.pool.begin().await?;
        let result = lock_cli_device(&mut tx, actor).await?;
        sqlx::query("UPDATE app.cli_devices SET last_used_at=clock_timestamp() WHERE id=$1")
            .bind(result.id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cli_devices(&self, login_id: Id) -> Result<Vec<CliDevice>, StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::authority::browser(&mut tx, &Actor::Browser { login_id }, false).await?;
        let rows = sqlx::query("SELECT id,name,created_at,last_used_at FROM app.cli_devices WHERE revoked_at IS NULL ORDER BY created_at DESC,id")
            .fetch_all(&mut *tx).await?;
        let result = rows.iter().map(device).collect::<Result<_, _>>()?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn revoke_cli_device(&self, login_id: Id, id: Id) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::authority::browser(&mut tx, &Actor::Browser { login_id }, true).await?;
        let changed = sqlx::query("UPDATE app.cli_devices SET revoked_at=coalesce(revoked_at,clock_timestamp()) WHERE id=$1")
            .bind(id.as_uuid()).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn lock_password(tx: &mut Tx<'_>, snapshot: &AuthSnapshot) -> Result<(), StoreError> {
    let row = sqlx::query("SELECT session_epoch,password_verifier FROM app.operator_auth_state WHERE singleton FOR SHARE")
        .fetch_one(&mut **tx).await?;
    if snapshot.password_verifier.is_none()
        || row.try_get::<Option<String>, _>("password_verifier")? != snapshot.password_verifier
        || row.try_get::<i64, _>("session_epoch")? != snapshot.epoch.get() as i64
    {
        return Err(StoreError::InvalidCredentials);
    }
    Ok(())
}

async fn insert_login(tx: &mut Tx<'_>, remember: bool) -> Result<LoginAuthority, StoreError> {
    let row = sqlx::query("WITH moment AS (SELECT clock_timestamp() AS now) INSERT INTO app.browser_logins(auth_epoch,authenticated_at,expires_at) SELECT session_epoch,now,now + ($1 * interval '1 hour') FROM app.operator_auth_state,moment WHERE singleton RETURNING id,auth_epoch,authenticated_at,expires_at,clock_timestamp() AS now")
        .bind(if remember { 720_i32 } else { 12 }).fetch_one(&mut **tx).await?;
    authority(&row)
}

fn device(row: &sqlx::postgres::PgRow) -> Result<CliDevice, StoreError> {
    Ok(CliDevice {
        schema_version: SchemaV1,
        id: crate::db::id(row.try_get("id")?)?,
        name: row.try_get("name")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
    })
}

pub(crate) async fn lock_cli_device(
    tx: &mut Tx<'_>,
    actor: &Actor,
) -> Result<CliDevice, StoreError> {
    let Actor::OwnerDevice {
        device_id,
        verifier,
    } = actor
    else {
        return Err(StoreError::Forbidden);
    };
    let row = sqlx::query("SELECT id,name,created_at,last_used_at FROM app.cli_devices WHERE id=$1 AND verifier=$2 AND revoked_at IS NULL FOR UPDATE")
        .bind(device_id.as_uuid()).bind(verifier).fetch_optional(&mut **tx).await?.ok_or(StoreError::InvalidCredentials)?;
    device(&row)
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
