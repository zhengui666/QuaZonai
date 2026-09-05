//! Durable browser authority. This module consumes facts verified by the trusted
//! native-crypto adapter; it never accepts a public request's claimed TOTP step.
//! Auth row -> login -> device is the lock order for authoritative mutations.
use crate::{Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    auth::{BrowserSession, DeviceList, TrustedDevice},
    Id, Revision, SchemaV1,
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Clone)]
pub struct AuthSnapshot {
    pub initialized: bool,
    pub secret_ref: Option<Id>,
    pub epoch: Revision,
    pub database_now: DateTime<Utc>,
}
#[derive(Clone)]
pub struct BootstrapCapability {
    pub id: Id,
    pub verifier: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone)]
pub struct Enrollment {
    pub id: Id,
    pub secret_ref: Id,
    pub expires_at: DateTime<Utc>,
    pub database_now: DateTime<Utc>,
}
#[derive(Clone, Debug)]
pub struct LoginAuthority {
    pub id: Id,
    pub epoch: Revision,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub device_id: Option<Id>,
    pub database_now: DateTime<Utc>,
}
impl LoginAuthority {
    pub fn recent(&self) -> bool {
        self.authenticated_at + Duration::seconds(300) > self.database_now
    }
    pub fn public(&self) -> BrowserSession {
        BrowserSession {
            schema_version: SchemaV1,
            authenticated_at: self.authenticated_at,
            expires_at: self.expires_at,
            trusted_device_id: self.device_id,
            recent_authentication_required: !self.recent(),
        }
    }
}
#[derive(Clone, Copy)]
pub enum AuthOperation {
    Bootstrap,
    Login,
    Reauth,
}
impl AuthOperation {
    fn name(self) -> &'static str {
        match self {
            Self::Bootstrap => "BOOTSTRAP",
            Self::Login => "LOGIN",
            Self::Reauth => "REAUTH",
        }
    }
}
fn id(value: Uuid) -> Result<Id, StoreError> {
    Id::try_from(value.to_string()).map_err(|_| StoreError::Invalid("auth_identity"))
}
fn revision(value: i64) -> Result<Revision, StoreError> {
    Revision::try_from(value.to_string()).map_err(|_| StoreError::Invalid("auth_epoch"))
}
fn step_is_current(step: i64, now: DateTime<Utc>) -> bool {
    step >= 0 && (now.timestamp() / 30).abs_diff(step) <= 1
}
fn binding_valid(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn label_valid(trust: bool, label: Option<&str>) -> bool {
    match (trust, label) {
        (false, None) => true,
        (true, Some(value)) => {
            !value.trim().is_empty()
                && value.chars().count() <= 120
                && !value.chars().any(char::is_control)
        }
        _ => false,
    }
}

impl Store {
    pub async fn authentication_snapshot(&self) -> Result<AuthSnapshot, StoreError> {
        let row=sqlx::query("SELECT initialized,totp_secret_ref,session_epoch,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton")
            .fetch_one(&self.pool).await?;
        Ok(AuthSnapshot {
            initialized: row.try_get("initialized")?,
            secret_ref: row
                .try_get::<Option<String>, _>("totp_secret_ref")?
                .map(Id::try_from)
                .transpose()
                .map_err(|_| StoreError::Invalid("totp_secret_reference"))?,
            epoch: revision(row.try_get("session_epoch")?)?,
            database_now: row.try_get("now")?,
        })
    }

    /// Reserve attempts before expensive verification, even when it will fail.
    /// This uses one atomic DB operation rather than a per-process rate limiter.
    pub async fn reserve_auth_attempt(&self, operation: AuthOperation) -> Result<(), StoreError> {
        let admitted=sqlx::query("INSERT INTO app.auth_rate_windows(operation,window_started_at,attempts) VALUES($1,clock_timestamp(),1) ON CONFLICT(operation) DO UPDATE SET window_started_at=CASE WHEN app.auth_rate_windows.window_started_at + interval '60 seconds' <= clock_timestamp() THEN clock_timestamp() ELSE app.auth_rate_windows.window_started_at END, attempts=CASE WHEN app.auth_rate_windows.window_started_at + interval '60 seconds' <= clock_timestamp() THEN 1 ELSE app.auth_rate_windows.attempts+1 END WHERE app.auth_rate_windows.window_started_at + interval '60 seconds' <= clock_timestamp() OR app.auth_rate_windows.attempts < 5 RETURNING operation")
            .bind(operation.name()).fetch_optional(&self.pool).await?;
        if admitted.is_none() {
            let seconds:i32=sqlx::query_scalar("SELECT greatest(1,ceil(extract(epoch FROM window_started_at + interval '60 seconds' - clock_timestamp())))::integer FROM app.auth_rate_windows WHERE operation=$1")
                .bind(operation.name()).fetch_one(&self.pool).await?;
            return Err(StoreError::AuthRateLimited {
                retry_after_seconds: seconds.clamp(1, 60) as u32,
            });
        }
        Ok(())
    }

    /// Local operator CLI only; intentionally has no HTTP route.
    pub async fn issue_bootstrap_capability(
        &self,
        verifier: &str,
    ) -> Result<BootstrapCapability, StoreError> {
        if !verifier.starts_with("$argon2id$") || verifier.len() > 256 {
            return Err(StoreError::Invalid("bootstrap_verifier"));
        }
        let mut tx = self.pool.begin().await?;
        let initialized: bool = sqlx::query_scalar(
            "SELECT initialized FROM app.operator_auth_state WHERE singleton FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        if initialized {
            return Err(StoreError::SetupCompleted);
        }
        let row=sqlx::query("INSERT INTO app.bootstrap_capabilities(verifier,created_at,expires_at) VALUES($1,statement_timestamp(),statement_timestamp()+interval '15 minutes') RETURNING id,expires_at")
            .bind(verifier).fetch_one(&mut *tx).await?;
        let result = BootstrapCapability {
            id: id(row.try_get("id")?)?,
            verifier: verifier.into(),
            expires_at: row.try_get("expires_at")?,
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn bootstrap_challenge(
        &self,
        capability_id: Id,
    ) -> Result<BootstrapCapability, StoreError> {
        let row=sqlx::query("SELECT c.id,c.verifier,c.expires_at FROM app.bootstrap_capabilities c JOIN app.operator_auth_state a ON a.singleton WHERE c.id=$1 AND c.consumed_at IS NULL AND c.expires_at>clock_timestamp() AND NOT a.initialized")
            .bind(capability_id.as_uuid()).fetch_optional(&self.pool).await?.ok_or(StoreError::InvalidCredentials)?;
        Ok(BootstrapCapability {
            id: capability_id,
            verifier: row.try_get("verifier")?,
            expires_at: row.try_get("expires_at")?,
        })
    }

    /// expected_verifier is the exact record used by the native Argon2 check.
    /// The secret is already encrypted and synced; failed enrollment leaves only
    /// an unreferenced encrypted object, never published authentication authority.
    pub async fn start_enrollment(
        &self,
        capability_id: Id,
        expected_verifier: &str,
        secret_ref: Id,
        browser_binding: &str,
    ) -> Result<Enrollment, StoreError> {
        if !binding_valid(browser_binding) {
            return Err(StoreError::Invalid("browser_binding"));
        }
        let mut tx = self.pool.begin().await?;
        let initialized: bool = sqlx::query_scalar(
            "SELECT initialized FROM app.operator_auth_state WHERE singleton FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        if initialized {
            return Err(StoreError::SetupCompleted);
        }
        let consumed=sqlx::query("UPDATE app.bootstrap_capabilities SET consumed_at=clock_timestamp() WHERE id=$1 AND verifier=$2 AND consumed_at IS NULL AND expires_at>clock_timestamp() RETURNING id")
            .bind(capability_id.as_uuid()).bind(expected_verifier).fetch_optional(&mut *tx).await?;
        if consumed.is_none() {
            return Err(StoreError::InvalidCredentials);
        }
        let row=sqlx::query("INSERT INTO app.auth_enrollments(capability_id,secret_ref,browser_binding,created_at,expires_at) VALUES($1,$2,$3,statement_timestamp(),statement_timestamp()+interval '10 minutes') RETURNING id,expires_at,clock_timestamp() AS now")
            .bind(capability_id.as_uuid()).bind(secret_ref.as_uuid()).bind(browser_binding).fetch_one(&mut *tx).await?;
        let result = Enrollment {
            id: id(row.try_get("id")?)?,
            secret_ref,
            expires_at: row.try_get("expires_at")?,
            database_now: row.try_get("now")?,
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn enrollment_challenge(
        &self,
        enrollment_id: Id,
        browser_binding: &str,
    ) -> Result<Enrollment, StoreError> {
        if !binding_valid(browser_binding) {
            return Err(StoreError::InvalidCredentials);
        }
        let row=sqlx::query("SELECT e.secret_ref,e.expires_at,clock_timestamp() AS now FROM app.auth_enrollments e JOIN app.operator_auth_state a ON a.singleton WHERE e.id=$1 AND e.browser_binding=$2 AND e.confirmed_at IS NULL AND e.expires_at>clock_timestamp() AND NOT a.initialized")
            .bind(enrollment_id.as_uuid()).bind(browser_binding).fetch_optional(&self.pool).await?.ok_or(StoreError::InvalidCredentials)?;
        Ok(Enrollment {
            id: enrollment_id,
            secret_ref: id(row.try_get("secret_ref")?)?,
            expires_at: row.try_get("expires_at")?,
            database_now: row.try_get("now")?,
        })
    }

    pub async fn confirm_enrollment(
        &self,
        enrollment_id: Id,
        browser_binding: &str,
        expected_secret: Id,
        verified_step: i64,
        trust: bool,
        label: Option<&str>,
    ) -> Result<LoginAuthority, StoreError> {
        if !binding_valid(browser_binding) || !label_valid(trust, label) {
            return Err(StoreError::Invalid("enrollment_input"));
        }
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT initialized,session_epoch,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton FOR UPDATE").fetch_one(&mut *tx).await?;
        if row.try_get::<bool, _>("initialized")? {
            return Err(StoreError::SetupCompleted);
        }
        let epoch: i64 = row.try_get("session_epoch")?;
        let now: DateTime<Utc> = row.try_get("now")?;
        if !step_is_current(verified_step, now) {
            return Err(StoreError::InvalidCredentials);
        }
        let enrolled=sqlx::query("UPDATE app.auth_enrollments SET confirmed_at=$4 WHERE id=$1 AND browser_binding=$2 AND secret_ref=$3 AND confirmed_at IS NULL AND expires_at>$4 RETURNING id")
            .bind(enrollment_id.as_uuid()).bind(browser_binding).bind(expected_secret.as_uuid()).bind(now).fetch_optional(&mut *tx).await?;
        if enrolled.is_none() {
            return Err(StoreError::InvalidCredentials);
        }
        sqlx::query("UPDATE app.operator_auth_state SET initialized=true,totp_secret_ref=$1,last_accepted_totp_step=$2,setup_completed_at=$3 WHERE singleton")
            .bind(expected_secret.to_string()).bind(verified_step).bind(now).execute(&mut *tx).await?;
        let result = create_login(&mut tx, epoch, now, trust, label).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn login_with_verified_step(
        &self,
        snapshot: &AuthSnapshot,
        verified_step: i64,
        trust: bool,
        label: Option<&str>,
    ) -> Result<LoginAuthority, StoreError> {
        if !label_valid(trust, label) {
            return Err(StoreError::Invalid("device_label"));
        }
        let mut tx = self.pool.begin().await?;
        let (epoch, now) = consume_step(&mut tx, snapshot, verified_step).await?;
        let result = create_login(&mut tx, epoch, now, trust, label).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn browser_authority(&self, login_id: Id) -> Result<LoginAuthority, StoreError> {
        let row=sqlx::query("SELECT l.id,l.auth_epoch,l.authenticated_at,l.expires_at,l.device_id,clock_timestamp() AS now FROM app.browser_logins l JOIN app.operator_auth_state a ON a.singleton LEFT JOIN app.trusted_devices d ON d.id=l.device_id WHERE l.id=$1 AND a.initialized AND l.auth_epoch=a.session_epoch AND l.revoked_at IS NULL AND l.expires_at>clock_timestamp() AND (l.device_id IS NULL OR (d.revoked_at IS NULL AND d.expires_at>clock_timestamp() AND d.auth_epoch=a.session_epoch))")
            .bind(login_id.as_uuid()).fetch_optional(&self.pool).await?.ok_or(StoreError::AuthenticationRequired)?;
        authority(&row)
    }

    pub async fn reauthenticate(
        &self,
        login_id: Id,
        snapshot: &AuthSnapshot,
        verified_step: i64,
    ) -> Result<LoginAuthority, StoreError> {
        let mut tx = self.pool.begin().await?;
        let (_, now) = consume_step(&mut tx, snapshot, verified_step).await?;
        lock_login(&mut tx, login_id, false).await?;
        let row=sqlx::query("UPDATE app.browser_logins SET authenticated_at=$2 WHERE id=$1 RETURNING id,auth_epoch,authenticated_at,expires_at,device_id,$2::timestamptz AS now")
            .bind(login_id.as_uuid()).bind(now).fetch_one(&mut *tx).await?;
        let result = authority(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn logout_browser(&self, login_id: Id) -> Result<(), StoreError> {
        sqlx::query("UPDATE app.browser_logins SET revoked_at=clock_timestamp() WHERE id=$1 AND revoked_at IS NULL").bind(login_id.as_uuid()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn trusted_devices(
        &self,
        login_id: Id,
        cursor: Option<Id>,
    ) -> Result<DeviceList, StoreError> {
        let current = self.browser_authority(login_id).await?;
        let rows=sqlx::query("SELECT id,label,last_used_at,expires_at,revoked_at FROM app.trusted_devices WHERE auth_epoch=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT 101")
            .bind(current.epoch.get() as i64).bind(cursor.map(Id::as_uuid)).fetch_all(&self.pool).await?;
        let mut items: Vec<TrustedDevice> = rows
            .into_iter()
            .map(|row| {
                Ok(TrustedDevice {
                    id: id(row.try_get("id")?)?,
                    label: row.try_get("label")?,
                    last_used_at: row.try_get("last_used_at")?,
                    expires_at: row.try_get("expires_at")?,
                    revoked_at: row.try_get("revoked_at")?,
                })
            })
            .collect::<Result<_, StoreError>>()?;
        let has_more = items.len() > 100;
        items.truncate(100);
        let next_cursor = if has_more {
            items.last().map(|item| item.id)
        } else {
            None
        };
        Ok(DeviceList {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }

    pub async fn revoke_trusted_device(
        &self,
        login_id: Id,
        device_id: Id,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM app.operator_auth_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
        let current = lock_login(&mut tx, login_id, true).await?;
        let exists = sqlx::query(
            "SELECT id FROM app.trusted_devices WHERE id=$1 AND auth_epoch=$2 FOR UPDATE",
        )
        .bind(device_id.as_uuid())
        .bind(current.epoch.get() as i64)
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query("UPDATE app.trusted_devices SET revoked_at=clock_timestamp() WHERE id=$1 AND revoked_at IS NULL").bind(device_id.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.browser_logins SET revoked_at=clock_timestamp() WHERE device_id=$1 AND revoked_at IS NULL").bind(device_id.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

fn authority(row: &sqlx::postgres::PgRow) -> Result<LoginAuthority, StoreError> {
    Ok(LoginAuthority {
        id: id(row.try_get("id")?)?,
        epoch: revision(row.try_get("auth_epoch")?)?,
        authenticated_at: row.try_get("authenticated_at")?,
        expires_at: row.try_get("expires_at")?,
        device_id: row
            .try_get::<Option<Uuid>, _>("device_id")?
            .map(id)
            .transpose()?,
        database_now: row.try_get("now")?,
    })
}

async fn lock_login(
    tx: &mut Tx<'_>,
    login_id: Id,
    recent: bool,
) -> Result<LoginAuthority, StoreError> {
    let row=sqlx::query("SELECT l.id,l.auth_epoch,l.authenticated_at,l.expires_at,l.device_id,clock_timestamp() AS now FROM app.browser_logins l JOIN app.operator_auth_state a ON a.singleton LEFT JOIN app.trusted_devices d ON d.id=l.device_id WHERE l.id=$1 AND a.initialized AND l.auth_epoch=a.session_epoch AND l.revoked_at IS NULL AND l.expires_at>clock_timestamp() AND (l.device_id IS NULL OR (d.revoked_at IS NULL AND d.expires_at>clock_timestamp() AND d.auth_epoch=a.session_epoch)) FOR UPDATE OF l")
        .bind(login_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::AuthenticationRequired)?;
    let result = authority(&row)?;
    if recent && !result.recent() {
        return Err(StoreError::RecentAuthenticationRequired);
    }
    Ok(result)
}

async fn consume_step(
    tx: &mut Tx<'_>,
    snapshot: &AuthSnapshot,
    verified_step: i64,
) -> Result<(i64, DateTime<Utc>), StoreError> {
    let row=sqlx::query("SELECT initialized,totp_secret_ref,session_epoch,last_accepted_totp_step,clock_timestamp() AS now FROM app.operator_auth_state WHERE singleton FOR UPDATE").fetch_one(&mut **tx).await?;
    let epoch: i64 = row.try_get("session_epoch")?;
    let now: DateTime<Utc> = row.try_get("now")?;
    if !row.try_get::<bool, _>("initialized")?
        || epoch as u64 != snapshot.epoch.get()
        || row.try_get::<Option<String>, _>("totp_secret_ref")?
            != snapshot.secret_ref.map(|id| id.to_string())
        || !step_is_current(verified_step, now)
    {
        return Err(StoreError::InvalidCredentials);
    }
    if row
        .try_get::<Option<i64>, _>("last_accepted_totp_step")?
        .is_some_and(|last| verified_step <= last)
    {
        return Err(StoreError::TotpReplay);
    }
    sqlx::query("UPDATE app.operator_auth_state SET last_accepted_totp_step=$1 WHERE singleton")
        .bind(verified_step)
        .execute(&mut **tx)
        .await?;
    Ok((epoch, now))
}

async fn create_login(
    tx: &mut Tx<'_>,
    epoch: i64,
    now: DateTime<Utc>,
    trust: bool,
    label: Option<&str>,
) -> Result<LoginAuthority, StoreError> {
    let login_id = Id::new();
    let expires_at = now
        + if trust {
            Duration::days(30)
        } else {
            Duration::hours(12)
        };
    let device_id = if trust {
        let row=sqlx::query("INSERT INTO app.trusted_devices(token_verifier_ref,label,last_used_at,expires_at,auth_epoch) VALUES($1,$2,$3,$4,$5) RETURNING id")
            .bind(format!("native-browser-login:{login_id}")).bind(label.ok_or(StoreError::Invalid("device_label"))?).bind(now).bind(expires_at).bind(epoch).fetch_one(&mut **tx).await?;
        Some(id(row.try_get("id")?)?)
    } else {
        None
    };
    sqlx::query("INSERT INTO app.browser_logins(id,auth_epoch,authenticated_at,expires_at,device_id) VALUES($1,$2,$3,$4,$5)")
        .bind(login_id.as_uuid()).bind(epoch).bind(now).bind(expires_at).bind(device_id.map(Id::as_uuid)).execute(&mut **tx).await?;
    Ok(LoginAuthority {
        id: login_id,
        epoch: revision(epoch)?,
        authenticated_at: now,
        expires_at,
        device_id,
        database_now: now,
    })
}
