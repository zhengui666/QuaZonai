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
    pub max_cpu: u16,
    pub max_memory_mib: u32,
    pub max_wall_seconds: u32,
    pub max_output_bytes: u64,
    pub max_parallel_jobs: u32,
    pub max_pending_jobs: u32,
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
        let bytes = files::read_file(path, 256 * 1024, false)?;
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
            || !(1..=1024).contains(&self.max_cpu)
            || !(64..=1_048_576).contains(&self.max_memory_mib)
            || !(1..=86_400).contains(&self.max_wall_seconds)
            || !(1..=64 * 1024 * 1024).contains(&self.max_output_bytes)
            || !(1..=32).contains(&self.max_parallel_jobs)
            || self.max_pending_jobs < self.max_parallel_jobs
            || self.max_pending_jobs > 4096
            || !(64 * 1024 * 1024..=1_099_511_627_776).contains(&self.storage_quota_bytes)
            || !(1..=8).contains(&self.images.len())
            || self.catalogs.len() > 256
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
            let raw = files::read_file(&registration.metadata_file, 1024 * 1024, false)?;
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
