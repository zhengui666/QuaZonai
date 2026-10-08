//! Deployment-selected resources. None of these host paths are accepted over the Runtime API.
use crate::{files, now, Failure, Result};
use contracts::{catalogs::RuntimeCatalogMetadataV1, runs::RunKind, SchemaV1};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRegistration {
    pub job_kind: RunKind,
    pub image_ref: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRegistration {
    pub root: PathBuf,
    pub metadata_file: PathBuf,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    pub schema_version: SchemaV1,
    pub state_dir: PathBuf,
    pub credential_file: PathBuf,
    pub docker_socket: PathBuf,
    pub bind: SocketAddr,
    pub images: Vec<ImageRegistration>,
    pub catalogs: Vec<CatalogRegistration>,
    pub max_cpu: Option<u32>,
    pub max_memory_mib: Option<u32>,
    pub max_wall_seconds: Option<u32>,
    /// Optional upper bound for explicitly requested output budgets; no default.
    #[serde(default)]
    pub max_output_bytes: Option<u64>,
    pub max_parallel_jobs: Option<u32>,
    pub max_pending_jobs: Option<u32>,
    pub storage_quota_bytes: u64,
}

#[derive(Clone)]
pub struct RegisteredCatalog {
    pub root: PathBuf,
    pub metadata: RuntimeCatalogMetadataV1,
    pub raw_metadata: Vec<u8>,
}

impl RuntimeConfig {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = files::read_file(path, usize::MAX, false)?;
        serde_json::from_slice(&bytes).map_err(|_| Failure::Invalid("runtime_configuration"))
    }
    pub fn validate(&self) -> Result<Vec<RegisteredCatalog>> {
        // Same-host TLS reverse proxy is the only production ingress. The control plane
        // still validates its configured HTTPS origin and CA; this listener is not public.
        if !self.bind.ip().is_loopback()
            || self.bind.port() == 0
            || !self.state_dir.is_absolute()
            || !self.credential_file.is_absolute()
            || !self.docker_socket.is_absolute()
            || self.max_cpu == Some(0)
            || self.max_memory_mib == Some(0)
            || self.max_wall_seconds == Some(0)
            || self.max_output_bytes.is_some_and(|bytes| bytes == 0 || bytes > i64::MAX as u64)
            || self.max_parallel_jobs == Some(0)
            || self.max_pending_jobs.zip(self.max_parallel_jobs).is_some_and(|(pending, parallel)| pending < parallel)
            || self.max_pending_jobs == Some(0)
            || self.storage_quota_bytes == 0 || self.storage_quota_bytes > i64::MAX as u64
            || !(1..=8).contains(&self.images.len())
        {
            return Err(Failure::Invalid("runtime_configuration"));
        }
        let mut kinds = BTreeSet::new();
        for image in &self.images {
            if !matches!(
                image.job_kind,
                RunKind::DataValidate
                    | RunKind::AlphaEvaluate
                    | RunKind::PortfolioBuild
                    | RunKind::PortfolioSimulate
                    | RunKind::ForwardEvaluate
            ) || !kinds.insert(serde_json::to_string(&image.job_kind)?)
                || !domain::runtime::pinned_image(&image.image_ref)
            {
                return Err(Failure::Invalid("runtime_image_registration"));
            }
        }
        let mut catalogs = Vec::with_capacity(self.catalogs.len());
        let mut identities = BTreeSet::new();
        for registration in &self.catalogs {
            let root = files::canonical_directory(&registration.root)?;
            if root.starts_with(&self.state_dir)
                || self.state_dir.starts_with(&root)
                || self.credential_file.starts_with(&root)
                || self.docker_socket.starts_with(&root)
                || catalogs.iter().any(|registered: &RegisteredCatalog| {
                    root.starts_with(&registered.root) || registered.root.starts_with(&root)
                })
            {
                return Err(Failure::Invalid("catalog_root_overlap"));
            }
            let raw = files::read_file(&registration.metadata_file, usize::MAX, false)?;
            let metadata: RuntimeCatalogMetadataV1 =
                serde_json::from_slice(&raw).map_err(|_| Failure::Invalid("catalog_metadata"))?;
            domain::catalogs::metadata(&metadata, now())
                .map_err(|_| Failure::Invalid("catalog_metadata"))?;
            if !identities.insert((
                metadata.registered_ref.clone(),
                metadata.storage_version.clone(),
            )) {
                return Err(Failure::Invalid("catalog_identity"));
            }
            catalogs.push(RegisteredCatalog {
                root,
                metadata,
                raw_metadata: raw,
            });
        }
        Ok(catalogs)
    }
}

pub fn credential(path: &Path) -> Result<String> {
    let bytes = files::read_file(path, 8193, true)?;
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
    let value = std::str::from_utf8(bytes).map_err(|_| Failure::Invalid("runtime_credential"))?;
    domain::settings::secret_value(
        contracts::settings::IntegrationSecretPurpose::Runtime,
        value,
    )
    .map_err(|_| Failure::Invalid("runtime_credential"))?;
    Ok(value.to_owned())
}

#[cfg(test)]
mod output_capacity_tests {
    use super::*;
    #[test]
    fn omitted_or_null_capacity_is_unlimited_and_legacy_value_is_preserved() {
        let mut value = serde_json::json!({
            "schema_version":1,"state_dir":"/state","credential_file":"/credential",
            "docker_socket":"/socket","bind":"127.0.0.1:8080","images":[],"catalogs":[],
            "max_cpu":1,"max_memory_mib":512,"max_wall_seconds":60,
            "max_parallel_jobs":1,"max_pending_jobs":4,"storage_quota_bytes":134217728
        });
        let absent: RuntimeConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(absent.max_output_bytes, None);
        value["max_output_bytes"] = serde_json::Value::Null;
        let unlimited: RuntimeConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(unlimited.max_output_bytes, None);
        value["max_output_bytes"] = serde_json::json!(67108864);
        let legacy: RuntimeConfig = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.max_output_bytes, Some(67108864));
    }
}

#[cfg(test)]
mod resource_ceiling_tests {
    use super::*;
    use serde_json::{json, Value};

    fn config() -> Value {
        json!({"schema_version":1,"state_dir":"/tmp/qz-state",
            "credential_file":"/tmp/qz-credential","docker_socket":"/tmp/qz-docker",
            "bind":"127.0.0.1:18973","catalogs":[],
            "images":[{"job_kind":"DATA_VALIDATE","image_ref":format!("sha256:{}", "a".repeat(64))}],
            "max_output_bytes":67108864,"storage_quota_bytes":134217728})
    }

    #[test]
    fn omitted_and_null_resource_ceilings_are_not_finite_defaults() {
        for null in [false, true] {
            let mut value = config();
            if null {
                for field in ["max_cpu", "max_memory_mib", "max_parallel_jobs", "max_pending_jobs"] {
                    value[field] = Value::Null;
                }
            }
            let parsed: RuntimeConfig = serde_json::from_value(value).unwrap();
            assert!(parsed.max_cpu.is_none() && parsed.max_memory_mib.is_none());
            assert!(parsed.max_parallel_jobs.is_none() && parsed.max_pending_jobs.is_none());
            parsed.validate().unwrap();
        }
    }

    #[test]
    fn explicit_resource_choices_keep_numeric_bounds_without_product_clamps() {
        let mut value = config();
        for field in ["max_cpu", "max_memory_mib", "max_parallel_jobs", "max_pending_jobs"] {
            value[field] = json!(u32::MAX);
        }
        serde_json::from_value::<RuntimeConfig>(value.clone()).unwrap().validate().unwrap();
        for field in ["max_cpu", "max_memory_mib", "max_parallel_jobs", "max_pending_jobs"] {
            let mut zero = value.clone();
            zero[field] = json!(0);
            assert!(serde_json::from_value::<RuntimeConfig>(zero).unwrap().validate().is_err());
            let mut overflow = value.clone();
            overflow[field] = json!(u64::from(u32::MAX) + 1);
            assert!(serde_json::from_value::<RuntimeConfig>(overflow).is_err());
        }
        value["max_parallel_jobs"] = json!(33);
        value["max_pending_jobs"] = json!(32);
        assert!(serde_json::from_value::<RuntimeConfig>(value).unwrap().validate().is_err());
    }
}
