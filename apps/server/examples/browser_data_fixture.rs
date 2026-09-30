//! Test-only controlled metadata/TLS peer for real browser/API admission.
//! No original market data, production Runtime, OCI execution or qualification.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod metadata_fixture;
#[path = "../tests/support/runtime_native.rs"]
mod native;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(args.next().ok_or("Supply one new private output path")?);
    if args.next().is_some() || !output.is_absolute() {
        return Err("Supply one new absolute private output path".into());
    }
    let document = metadata_fixture::calendar_metadata();
    let target = format!(
        "/runtime/v1/catalogs/{}/metadata?storage_version={}",
        document.registered_ref, document.storage_version
    );
    let tls = native::native_tls_catalog(target, serde_json::to_vec(&document)?, None).await;
    let value = serde_json::json!({
        "schema_version":1,
        "scope":"controlled synthetic TLS metadata; admission only, no OCI execution",
        "endpoint":tls.endpoint(),
        "runtime_targets":[{"origin":tls.endpoint(),"addresses":[tls.server.address.to_string()]}],
        "credential":native::SECRET,
        "ca_pem":String::from_utf8(tls.ca.clone())?,
        "metadata":document,
    });
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(output)?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    // Harness owns this process group and terminates it after its private API.
    // Keep the native listener and its temporary TLS key lease alive meanwhile.
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    drop(tls);
    Ok(())
}
