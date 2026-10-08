//! Human-authorized integration setup. Secret material is write-only and stays
//! in the native vault; creating configuration never performs a network probe.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    Json,
};
use contracts::{
    control::{CommandResult, ListQuery, Page},
    settings::*,
    Id,
};
use integrations::secrets::{SecretError, SecretVault};
use std::{future::Future, sync::Arc};
use store::{settings::NativeSecretBinding, StoreError};

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}
fn query(value: Result<Query<ListQuery>, QueryRejection>) -> Result<ListQuery, ApiError> {
    value
        .map(|Query(value)| value)
        .map_err(|_| ApiError::validation())
}
fn development(state: &AppState, requested: bool) -> Result<(), ApiError> {
    if requested && state.policy.secure {
        return Err(StoreError::Domain(domain::research::invalid(
            "configuration.development_http",
            "DEPLOYMENT_DISALLOWS_HTTP",
        ))
        .into());
    }
    Ok(())
}

/// Keep ownership of an admitted command after client disconnection.
/// This is not a queue or retry engine: the same original transaction runs once.
pub(crate) async fn command<T, F>(_state: &AppState, operation: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, StoreError>> + Send + 'static,
{
    tokio::spawn(operation)
    .await
    .map_err(|_| ApiError::internal())?
    .map_err(Into::into)
}

fn material(purpose: IntegrationSecretPurpose, value: &[u8]) -> Result<(), StoreError> {
    let value = std::str::from_utf8(value)
        .map_err(|_| domain::research::invalid("value", "INVALID_SECRET_MATERIAL"))?;
    domain::settings::secret_value(purpose, value)?;
    if purpose == IntegrationSecretPurpose::TlsCa {
        let invalid = || domain::research::invalid("value", "INVALID_CA_CERTIFICATE");
        let certificates =
            reqwest::Certificate::from_pem_bundle(value.as_bytes()).map_err(|_| invalid())?;
        if certificates.is_empty() {
            return Err(invalid().into());
        }
        // rustls validates each DER trust anchor during native Client construction.
        // Building the client does not resolve a host or open a network connection.
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .tls_built_in_root_certs(false);
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
        builder.build().map_err(|_| invalid())?;
    }
    Ok(())
}

#[cfg(test)]
mod certificate_capacity_tests {
    use super::*;

    #[test]
    fn native_ca_material_crosses_former_certificate_count_limit() {
        // Public test root only. No private key or system trust store is used.
        let certificate = include_str!("../tests/fixtures/capacity-ca.pem");
        let bundle = certificate.repeat(17);
        assert!(bundle.len() < 65536);
        assert_eq!(reqwest::Certificate::from_pem_bundle(bundle.as_bytes()).unwrap().len(), 17);
        material(IntegrationSecretPurpose::TlsCa, bundle.as_bytes()).unwrap();
        for invalid in [b"".as_slice(), b"not a PEM certificate", b"-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----"] {
            assert!(material(IntegrationSecretPurpose::TlsCa, invalid).is_err());
        }
    }
}

async fn native_references(
    vault: Arc<SecretVault>,
    refs: Vec<NativeSecretBinding>,
) -> Result<(), StoreError> {
    tokio::task::spawn_blocking(move || {
        for reference in refs {
            let value = vault
                .read(reference.id, reference.purpose.code())
                .map_err(|_| {
                    domain::research::invalid("credential_ref", "NATIVE_REFERENCE_UNAVAILABLE")
                })?;
            material(reference.purpose, &value)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| StoreError::SecretCleanup)?
}

#[utoipa::path(post,path="/api/v2/settings/credentials",tag="Integration settings",request_body=IntegrationSecretCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<IntegrationSecretView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn register_secret(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<IntegrationSecretCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<IntegrationSecretView>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let vault = state.vault.clone();
    let result = command(&state, async move {
        let intent = request.intent;
        let value = request.value;
        store
            .register_integration_secret(
                &actor,
                &key,
                &intent,
                move |id, purpose, replayed| async move {
                    tokio::task::spawn_blocking(move || {
                        material(purpose, value.as_bytes())?;
                        if replayed {
                            let original = vault
                                .read(id, purpose.code())
                                .map_err(|_| StoreError::SecretCleanup)?;
                            if original != value.as_bytes() {
                                return Err(StoreError::IdempotencyConflict);
                            }
                            return Ok(());
                        }
                        match vault.put_at(id, purpose.code(), value.as_bytes()) {
                            Ok(_) => Ok(()),
                            // A prior request can publish the immutable object before its
                            // database receipt. Only native authenticated exact bytes may
                            // be adopted for the same server-allocated command identity.
                            Err(SecretError::Io(error))
                                if error.kind() == std::io::ErrorKind::AlreadyExists =>
                            {
                                let original = vault
                                    .read(id, purpose.code())
                                    .map_err(|_| StoreError::SecretCleanup)?;
                                if original == value.as_bytes() {
                                    Ok(())
                                } else {
                                    Err(StoreError::IdempotencyConflict)
                                }
                            }
                            Err(_) => Err(StoreError::SecretCleanup),
                        }
                    })
                    .await
                    .map_err(|_| StoreError::SecretCleanup)?
                },
            )
            .await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/integrations/runtimes",tag="Integration settings",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<RuntimeView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn runtimes(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<RuntimeView>>, ApiError> {
    Ok(Json(state.store.runtimes(&actor, &query(q)?).await?))
}

#[utoipa::path(get,path="/api/v2/integrations/runtimes/{id}",tag="Integration settings",params(("id"=Id,Path)),responses((status=200,body=RuntimeView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn runtime(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<RuntimeView>, ApiError> {
    Ok(Json(state.store.runtime(&actor, path(id)?).await?))
}

#[utoipa::path(post,path="/api/v2/integrations/runtimes",tag="Integration settings",request_body=RuntimeCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<RuntimeView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create_runtime(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<RuntimeCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<RuntimeView>>), ApiError> {
    let request = json(body)?;
    development(&state, request.configuration.development_http)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let vault = state.vault.clone();
    let result = command(&state, async move {
        store
            .create_runtime(&actor, &key, &request, move |refs| {
                native_references(vault, refs)
            })
            .await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(patch,path="/api/v2/integrations/runtimes/{id}",tag="Integration settings",request_body=RuntimeUpdate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<RuntimeView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn update_runtime(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<RuntimeUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<RuntimeView>>, ApiError> {
    let request = json(body)?;
    let id = path(id)?;
    development(&state, request.configuration.development_http)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let vault = state.vault.clone();
    Ok(Json(
        command(&state, async move {
            store
                .update_runtime(&actor, &key, id, &request, move |refs| {
                    native_references(vault, refs)
                })
                .await
        })
        .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/integrations/downstreams",tag="Integration settings",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<DownstreamView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn downstreams(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<DownstreamView>>, ApiError> {
    Ok(Json(state.store.downstreams(&actor, &query(q)?).await?))
}

#[utoipa::path(get,path="/api/v2/integrations/downstreams/{id}",tag="Integration settings",params(("id"=Id,Path)),responses((status=200,body=DownstreamView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn downstream(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<DownstreamView>, ApiError> {
    Ok(Json(state.store.downstream(&actor, path(id)?).await?))
}

#[utoipa::path(post,path="/api/v2/integrations/downstreams",tag="Integration settings",request_body=DownstreamCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<DownstreamView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create_downstream(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<DownstreamCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<DownstreamView>>), ApiError> {
    let request = json(body)?;
    development(&state, request.configuration.development_http)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let vault = state.vault.clone();
    let result = command(&state, async move {
        store
            .create_downstream(&actor, &key, &request, move |refs| {
                native_references(vault, refs)
            })
            .await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(patch,path="/api/v2/integrations/downstreams/{id}",tag="Integration settings",request_body=DownstreamUpdate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<DownstreamView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn update_downstream(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<DownstreamUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<DownstreamView>>, ApiError> {
    let request = json(body)?;
    let id = path(id)?;
    development(&state, request.configuration.development_http)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let vault = state.vault.clone();
    Ok(Json(
        command(&state, async move {
            store
                .update_downstream(&actor, &key, id, &request, move |refs| {
                    native_references(vault, refs)
                })
                .await
        })
        .await?,
    ))
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn five_admitted_commands_keep_ownership_when_a_waiter_disconnects() {
        let root = tempfile::tempdir().unwrap();
        let secrets = root.path().join("secrets");
        std::fs::create_dir(&secrets).unwrap();
        std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o700)).unwrap();
        let key = root.path().join("key");
        SecretVault::initialize_key(&key).unwrap();
        // No database operation occurs in this deterministic ownership test.
        let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://fixture:fixture@localhost/unused").unwrap();
        let state = AppState::new(store::Store::from_pool(pool), SecretVault::open(&secrets, &key).unwrap(),
            crate::WebPolicy::new("https://localhost", "127.0.0.1:8080".parse().unwrap(), false).unwrap());
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut waiters = Vec::new();
        let mut releases = Vec::new();
        for _ in 0..5 {
            let state = state.clone();
            let completed = completed.clone();
            let (entered, observed) = tokio::sync::oneshot::channel();
            let (release, released) = tokio::sync::oneshot::channel();
            waiters.push(tokio::spawn(async move {
                command(&state, async move {
                    entered.send(()).unwrap();
                    released.await.unwrap();
                    completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<_, StoreError>(())
                }).await
            }));
            tokio::time::timeout(std::time::Duration::from_secs(3), observed).await.unwrap().unwrap();
            releases.push(release);
        }
        let disconnected = waiters.pop().unwrap();
        disconnected.abort();
        assert!(disconnected.await.unwrap_err().is_cancelled());
        for release in releases { release.send(()).unwrap(); }
        for waiter in waiters { waiter.await.unwrap().unwrap(); }
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while completed.load(std::sync::atomic::Ordering::SeqCst) != 5 { tokio::task::yield_now().await; }
        }).await.unwrap();
    }
}
