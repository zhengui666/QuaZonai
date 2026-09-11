//! Immutable inputs into fixed native mounts, and bounded job-owned output adoption.
use crate::{
    config::RegisteredCatalog,
    engine,
    files::{self, RuntimeRoot},
    journal::Journal,
    Failure, Result,
};
use bollard::models::Mount;
use contracts::{execution::*, runtime_jobs::*, Id};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::PathBuf,
    sync::Arc,
};

mod cache;
pub use cache::{cleanup_terminal, recover};

pub struct Materialized {
    pub output: PathBuf,
    pub mounts: Vec<Mount>,
}

pub fn registered<'a>(
    catalogs: &'a [RegisteredCatalog],
    reference: &str,
    version: &str,
) -> Result<&'a RegisteredCatalog> {
    catalogs
        .iter()
        .find(|catalog| {
            catalog.metadata.registered_ref == reference
                && catalog.metadata.storage_version == version
        })
        .ok_or(Failure::Invalid("catalog_binding"))
}

pub async fn parameters(
    journal: &Journal,
    spec: &JobSpecV1,
    catalogs: &[RegisteredCatalog],
) -> Result<NativeTaskParametersV1> {
    let (_, bytes) = journal.input_object(spec.parameters_artifact_id).await?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Failure::Invalid("parameters_size"));
    }
    let parameters: NativeTaskParametersV1 =
        serde_json::from_slice(&bytes).map_err(|_| Failure::Invalid("native_parameters"))?;
    domain::execution::task(spec, &parameters)
        .map_err(|_| Failure::Invalid("native_task_binding"))?;
    for input in &spec.inputs {
        if let RuntimeInputV1::Dataset {
            registered_ref,
            storage_version,
            role,
            ..
        } = input
        {
            let catalog = registered(catalogs, registered_ref, storage_version)?;
            if catalog.metadata.partition != *role {
                return Err(Failure::Invalid("catalog_partition"));
            }
        }
    }
    let selections: Vec<_> = match &parameters {
        NativeTaskParametersV1::ValidateData { selections, .. } => selections
            .iter()
            .map(|selected| (selected.dataset_revision_id, &selected.selection))
            .collect(),
        NativeTaskParametersV1::EvaluateAlpha {
            dataset_revision_id,
            request,
            ..
        } => {
            vec![(*dataset_revision_id, &request.selection)]
        }
        NativeTaskParametersV1::SimulatePortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            vec![(*dataset_revision_id, &request.selection)]
        }
        NativeTaskParametersV1::CompileModel { .. }
        | NativeTaskParametersV1::BuildPortfolio { .. } => Vec::new(),
    };
    for (revision, selection) in selections {
        let catalog = spec
            .inputs
            .iter()
            .find_map(|input| match input {
                RuntimeInputV1::Dataset {
                    revision_id,
                    registered_ref,
                    storage_version,
                    ..
                } if *revision_id == revision => Some((registered_ref, storage_version)),
                _ => None,
            })
            .ok_or(Failure::Invalid("catalog_binding"))?;
        selection_scope(registered(catalogs, catalog.0, catalog.1)?, selection)?;
    }
    Ok(parameters)
}

fn selection_scope(
    catalog: &RegisteredCatalog,
    selection: &contracts::science::NativeBarSelectionV1,
) -> Result<()> {
    use nautilus_model::data::BarType;
    let [attested] = catalog.metadata.quality.datasets.as_slice() else {
        return Err(Failure::Invalid("catalog_quality_scope"));
    };
    if selection.event_start_ns < attested.selection.event_start_ns
        || selection.event_end_ns > attested.selection.event_end_ns
    {
        return Err(Failure::Invalid("catalog_event_scope"));
    }
    if selection.decision_cutoff_ns > attested.selection.decision_cutoff_ns {
        return Err(Failure::Invalid("catalog_cutoff_scope"));
    }
    for name in &selection.bar_types {
        // Use the upstream identity parser, not a second grammar based on string splitting.
        let native: BarType = name
            .parse()
            .map_err(|_| Failure::Invalid("catalog_bar_type_scope"))?;
        if native.to_string() != *name || !attested.selection.bar_types.contains(name) {
            return Err(Failure::Invalid("catalog_bar_type_scope"));
        }
        let instrument = native.instrument_id().to_string();
        if !attested.instrument_ids.contains(&instrument)
            || !catalog
                .metadata
                .universe
                .membership
                .iter()
                .any(|member| member.instrument_id == instrument)
        {
            return Err(Failure::Invalid("catalog_instrument_scope"));
        }
    }
    Ok(())
}

pub async fn inputs(
    root: Arc<RuntimeRoot>,
    journal: &Journal,
    spec: &JobSpecV1,
    catalogs: &[RegisteredCatalog],
) -> Result<Materialized> {
    parameters(journal, spec, catalogs).await?;
    let mut objects = BTreeMap::new();
    let mut total = 0u64;
    for input in &spec.inputs {
        if let RuntimeInputV1::Artifact {
            artifact_id,
            storage_version,
            byte_count,
            ..
        } = input
        {
            let (version, bytes) = journal.input_object(*artifact_id).await?;
            if version != *storage_version || bytes.len() as u64 != byte_count.get() {
                return Err(Failure::Invalid("input_object_binding"));
            }
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or(Failure::Capacity)?;
            objects.insert(*artifact_id, bytes);
        }
    }
    if let std::collections::btree_map::Entry::Vacant(entry) =
        objects.entry(spec.parameters_artifact_id)
    {
        let (_, bytes) = journal.input_object(spec.parameters_artifact_id).await?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(Failure::Capacity)?;
        entry.insert(bytes);
    }
    if total > domain::runtime_jobs::MAX_INPUT_OBJECTS_BYTES {
        return Err(Failure::Capacity);
    }
    let mut mounts = Vec::new();
    let mut datasets = Vec::new();
    for input in &spec.inputs {
        if let RuntimeInputV1::Dataset {
            revision_id,
            registered_ref,
            storage_version,
            ..
        } = input
        {
            let catalog = registered(catalogs, registered_ref, storage_version)?;
            // Registrations are immutable for this process. A restart must bind the same
            // native version or leave the old job unavailable, not substitute newer data.
            files::canonical_directory(&catalog.root)?;
            datasets.push(*revision_id);
            mounts.push(engine::bind(
                &catalog.root,
                &format!("/input/catalogs/{revision_id}"),
                true,
            )?);
        }
    }
    // This covers the additional filesystem copies as well as bounded output
    // staging. The immutable SQLite objects already consume their own quota.
    journal.reserve_materialization(spec).await?;
    let spec = spec.clone();
    tokio::task::spawn_blocking(move || {
        let destination = root.job(spec.run_id, spec.attempt_no);
        let encoded = serde_json::to_vec(&spec)?;
        if destination.try_exists()? {
            files::canonical_directory(&destination)?;
            let input = files::directory_handle(&destination.join("input"))?;
            if files::read_child(&input, "spec.json", 1024 * 1024)? != encoded {
                return Err(Failure::Integrity);
            }
            let directory = files::directory_handle(&destination.join("input/objects"))?;
            for (id, bytes) in &objects {
                if files::read_child(&directory, &id.to_string(), bytes.len())? != *bytes {
                    return Err(Failure::Integrity);
                }
            }
        } else {
            // The exact reserved slot is never an OCI mount. An interrupted
            // partial copy is discarded rather than allocating unbounded siblings.
            cache::discard_staging(&root, &spec)?;
            let staging = cache::staging(&root, &spec);
            files::private_directory(&staging)?;
            let input = staging.join("input");
            files::private_directory(&input)?;
            files::private_directory(&input.join("objects"))?;
            files::private_directory(&input.join("catalogs"))?;
            files::write_new(&input.join("spec.json"), &encoded)?;
            for (id, bytes) in &objects {
                files::write_new(&input.join("objects").join(id.to_string()), bytes)?;
            }
            for id in datasets {
                let path = input.join("catalogs").join(id.to_string());
                files::private_directory(&path)?;
                files::readonly_directory(&path)?;
            }
            files::readonly_directory(&input.join("objects"))?;
            files::readonly_directory(&input.join("catalogs"))?;
            files::readonly_directory(&input)?;
            files::writable_output(&staging.join("output"))?;
            File::open(&staging)?.sync_all()?;
            // No overwrite: a concurrent or previous materialization is never repaired
            // by replacing its path. The sole supervisor owns the native file lock.
            files::publish_directory(&staging, &destination)?;
        }
        mounts.insert(0, engine::bind(&destination.join("input"), "/input", true)?);
        let output = destination.join("output");
        files::canonical_directory(&output)?;
        mounts.push(engine::bind(&output, "/output", false)?);
        Ok(Materialized { output, mounts })
    })
    .await
    .map_err(|_| Failure::Integrity)?
}

fn json_schema(bytes: &[u8], output: &RuntimeOutputV1) -> Result<()> {
    domain::execution::output_shape(output, bytes)
        .map_err(|_| Failure::Invalid("native_output_schema"))
}

/// Native execution is already stopped. Reopening exact object IDs does not run code.
/// Schema-valid reports remain unqualified raw evidence until the independent evaluator.
pub fn outputs(root: &RuntimeRoot, spec: &JobSpecV1) -> Result<Vec<(RuntimeOutputV1, Vec<u8>)>> {
    let path = root.job(spec.run_id, spec.attempt_no).join("output");
    let directory = files::directory_handle(&path)?;
    let index: NativeJobOutputIndexV1 =
        serde_json::from_slice(&files::read_child(&directory, "index.json", 1024 * 1024)?)?;
    if index.artifacts.is_empty() || index.artifacts.len() > 64 {
        return Err(Failure::Invalid("native_output_index"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut schemas = std::collections::BTreeSet::new();
    let mut total = 0u64;
    let mut result = Vec::with_capacity(index.artifacts.len());
    for output in index.artifacts {
        if !seen.insert(output.storage_ref)
            || output.storage_version.get() != 1
            || output.schema.version != "1"
            || output.byte_count.get() == 0
            || !spec.requested_output_schemas.iter().any(|schema| {
                schema.name == output.schema.name && schema.version == output.schema.version
            })
            || !schemas.insert(output.schema.name.clone())
        {
            return Err(Failure::Invalid("native_output_index"));
        }
        total = total
            .checked_add(output.byte_count.get())
            .ok_or(Failure::Capacity)?;
        if total > spec.limits.output_bytes.get() {
            return Err(Failure::Capacity);
        }
        let maximum = usize::try_from(output.byte_count.get()).map_err(|_| Failure::Capacity)?;
        let bytes = files::read_child(&directory, &output.storage_ref.to_string(), maximum)?;
        if bytes.len() != maximum {
            return Err(Failure::Integrity);
        }
        json_schema(&bytes, &output)?;
        result.push((output, bytes));
    }
    if spec
        .requested_output_schemas
        .iter()
        .any(|schema| !schemas.contains(&schema.name))
    {
        return Err(Failure::Invalid("native_output_schema_missing"));
    }
    // Every byte under the writable output mount must be accounted for. A scratch
    // or truncated file left beside the index cannot escape the native output limit.
    for entry in fs::read_dir(&path)? {
        let name = entry?.file_name();
        let name = name
            .to_str()
            .ok_or(Failure::Invalid("native_output_name"))?;
        if name != "index.json" && !seen.iter().any(|id| id.to_string() == name) {
            return Err(Failure::Invalid("unlisted_native_output"));
        }
    }
    Ok(result)
}
