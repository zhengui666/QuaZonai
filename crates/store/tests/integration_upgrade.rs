//! Applied integration configuration is preserved; an absent historical CA is
//! not invented, downgraded to system trust, or allowed on a new write.
mod support;
use contracts::Id;
use sqlx::PgPool;
use store::Store;

#[sqlx::test(migrations = false)]
async fn upgrade_preserves_old_pinned_configuration_without_attesting_it(pool: PgPool) {
    support::migrate_before(&pool, 202609090020).await;
    let id = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'Historical runtime','https://runtime.example','PINNED_CA','historical-native-reference',ARRAY['DATA_VALIDATE'],'1',true)")
        .bind(id.as_uuid()).execute(&pool).await.unwrap();
    Store::from_pool(pool.clone()).migrate().await.unwrap();
    let stored: (String, Option<String>) = sqlx::query_as(
        "SELECT tls_policy,ca_certificate_ref FROM app.runtime_integrations WHERE id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, ("PINNED_CA".into(), None));
    let error =
        sqlx::query("UPDATE app.runtime_integrations SET name='Still missing CA' WHERE id=$1")
            .bind(id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    sqlx::query("UPDATE app.runtime_integrations SET ca_certificate_ref=$2 WHERE id=$1")
        .bind(id.as_uuid())
        .bind(Id::new().to_string())
        .execute(&pool)
        .await
        .unwrap();
}
