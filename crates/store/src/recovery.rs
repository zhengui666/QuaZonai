//! Offline restore cutover. No HTTP/MCP path; only the privileged migration identity.
use crate::{Store, StoreError};
use contracts::Id;
use sqlx::Row;

impl Store {
    pub async fn invalidate_restored_access(
        &self,
        recovery: Id,
    ) -> Result<serde_json::Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        let owner:bool=sqlx::query_scalar("SELECT pg_has_role(current_user,relowner,'USAGE') AND pg_has_role(session_user,relowner,'USAGE') FROM pg_class WHERE oid='app.operator_auth_state'::regclass").fetch_one(&mut *tx).await?;
        if !owner {
            return Err(StoreError::Forbidden);
        }
        // Writers must already be stopped; these locks settle earlier authority transactions.
        sqlx::query("LOCK TABLE app.operator_auth_state,app.browser_logins,app.trusted_devices,app.operator_command_grants,app.machine_credentials,app.machine_credential_revocations,app.cli_devices IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
        let auth=sqlx::query("SELECT id,initialized,session_epoch FROM app.operator_auth_state WHERE singleton FOR UPDATE").fetch_one(&mut *tx).await?;
        if !auth.try_get::<bool, _>("initialized")? {
            return Err(StoreError::AuthenticationRequired);
        }
        let replay:Option<serde_json::Value>=sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE principal_scope='SYSTEM_RECOVERY' AND operation='AUTH_RESTORE_INVALIDATE' AND idempotency_key=$1").bind(recovery.to_string()).fetch_optional(&mut *tx).await?;
        if let Some(replay) = replay {
            tx.commit().await?;
            return Ok(replay);
        }
        let epoch:i64=sqlx::query_scalar("SELECT greatest(session_epoch,coalesce((SELECT max(auth_epoch) FROM app.browser_logins),1),coalesce((SELECT max(auth_epoch) FROM app.trusted_devices),1),coalesce((SELECT max(auth_epoch) FROM app.operator_command_grants),1))+1 FROM app.operator_auth_state WHERE singleton").fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE app.operator_auth_state SET session_epoch=$1 WHERE singleton")
            .bind(epoch)
            .execute(&mut *tx)
            .await?;
        let revoked=sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) SELECT c.id,clock_timestamp(),'RESTORE_CUTOVER' FROM app.machine_credentials c WHERE NOT EXISTS(SELECT 1 FROM app.machine_credential_revocations r WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp())").execute(&mut *tx).await?.rows_affected();
        sqlx::query(
            "UPDATE app.cli_devices SET revoked_at=clock_timestamp() WHERE revoked_at IS NULL",
        )
        .execute(&mut *tx)
        .await?;
        let report = serde_json::json!({"schema_version":1,"recovery_id":recovery,"previous_epoch":auth.try_get::<i64,_>("session_epoch")?.to_string(),"new_epoch":epoch.to_string(),"revoked_machine_credentials":revoked.to_string()});
        sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES('SYSTEM_RECOVERY','AUTH_RESTORE_INVALIDATE',$1,$2,$3,200,$4)").bind(recovery.to_string()).bind(serde_json::json!({"schema_version":1,"recovery_id":recovery})).bind(auth.try_get::<uuid::Uuid,_>("id")?).bind(&report).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(report)
    }
}
