use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

/// Select the actual job package's non-dev resolved dependency graph. This is
/// build provenance, not a claim that another upstream version is API compatible.
pub fn from_metadata(
    metadata: &Value,
    manifest: &Path,
) -> Result<(String, String, String), String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("metadata packages missing")?;
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("metadata resolution missing")?;
    let manifest = manifest.canonicalize().map_err(|e| e.to_string())?;
    let roots: Vec<_> = packages
        .iter()
        .filter(|p| {
            p["manifest_path"]
                .as_str()
                .and_then(|s| Path::new(s).canonicalize().ok())
                .as_ref()
                == Some(&manifest)
        })
        .collect();
    if roots.len() != 1 {
        return Err("actual job package is missing or ambiguous".into());
    }
    let root = roots[0]["id"].as_str().ok_or("root package ID missing")?;
    let mut indexed = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().ok_or("dependency node ID missing")?;
        if indexed.insert(id, node).is_some() {
            return Err("duplicate dependency node ID".into());
        }
    }
    let mut selected = HashSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if !selected.insert(id) {
            continue;
        }
        let node = indexed.get(id).ok_or("selected dependency node missing")?;
        for dep in node["deps"].as_array().ok_or("dependency edges missing")? {
            let kinds = dep["dep_kinds"]
                .as_array()
                .ok_or("dependency kinds missing")?;
            if kinds
                .iter()
                .any(|kind| kind["kind"].is_null() || kind["kind"] == "build")
            {
                pending.push(dep["pkg"].as_str().ok_or("dependency package ID missing")?);
            }
        }
    }
    let models: Vec<_> = packages
        .iter()
        .filter(|p| {
            p["name"] == "nautilus-model"
                && p["id"].as_str().is_some_and(|id| selected.contains(id))
        })
        .collect();
    if models.len() != 1 {
        return Err("selected nautilus-model missing or ambiguous".into());
    }
    let model = models[0];
    let version = model["version"]
        .as_str()
        .ok_or("model version missing")?
        .to_owned();
    let source = match &model["source"] {
        Value::String(source) => source.clone(),
        Value::Null => {
            let path = Path::new(
                model["manifest_path"]
                    .as_str()
                    .ok_or("path manifest missing")?,
            )
            .canonicalize()
            .map_err(|e| e.to_string())?;
            let directory = path.parent().ok_or("path manifest parent missing")?;
            format!("path+file://{}", file_url_path(directory)?)
        }
        _ => return Err("model source invalid".into()),
    };
    let revision = if source.starts_with("git+") {
        let revision = source
            .rsplit_once('#')
            .ok_or("resolved git revision missing")?
            .1;
        if revision.is_empty() {
            return Err("resolved git revision empty".into());
        }
        revision.to_owned()
    } else if source.starts_with("registry+") || source.starts_with("path+file://") {
        String::new()
    } else {
        return Err("unsupported resolved model source".into());
    };
    Ok((version, revision, source))
}

fn file_url_path(path: &Path) -> Result<String, String> {
    let path = path.to_str().ok_or("non-UTF8 native path unsupported")?;
    let mut result = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(result)
}
