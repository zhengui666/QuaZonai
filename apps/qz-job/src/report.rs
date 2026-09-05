//! Atomic publication for compatibility reports, not the production artifact store.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        // Cleanup after failure or after publication is best-effort. A .pending
        // file is never the canonical report and must not be interpreted as one.
        let _ = fs::remove_file(&self.0);
    }
}

/// Publishes a complete, file-synchronized report without replacing an existing
/// destination. Only Linux filesystems supporting hard links are accepted. This
/// is not a claim of durable job completion or directory crash consistency.
pub fn write_probe_report<T: Serialize>(
    directory: &Path,
    name: &str,
    report: &T,
) -> io::Result<()> {
    write_with_sync(directory, name, report, File::sync_all)
}

fn write_with_sync<T: Serialize>(
    directory: &Path,
    name: &str,
    report: &T,
    sync: impl FnOnce(&File) -> io::Result<()>,
) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "INVALID_REPORT_NAME",
        ));
    }
    let destination = directory.join(name);
    let pending = directory.join(format!(".{name}.pending"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&pending)?;
    let guard = Pending(pending);
    serde_json::to_writer_pretty(&mut file, report).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    sync(&file)?;
    drop(file);
    // Link is an atomic create-if-absent operation on the same filesystem. Unlike
    // rename(), it cannot silently replace an existing destination. No fallible
    // evidence write follows this publication point.
    fs::hard_link(&guard.0, destination)
}

#[cfg(test)]
mod tests {
    use super::{write_probe_report, write_with_sync};
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde::{Serialize, Serializer};
    use serde_json::json;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..100 {
                let path = std::env::temp_dir().join(format!(
                    "qz-report-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                if fs::create_dir(&path).is_ok() {
                    return Self(path);
                }
            }
            panic!("could not create isolated report test directory")
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sync_failure_after_valid_json_does_not_publish_a_report() {
        let directory = TestDirectory::new();
        let error = write_with_sync(
            &directory.0,
            "report.json",
            &json!({"origin": "FIXTURE"}),
            |_| Err(io::Error::other("injected disk synchronization failure")),
        );
        assert!(error.is_err());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn serialization_failure_does_not_publish_a_report() {
        struct Invalid;
        impl Serialize for Invalid {
            fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("injected serialization failure"))
            }
        }
        let directory = TestDirectory::new();
        assert!(write_probe_report(&directory.0, "report.json", &Invalid).is_err());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn publication_never_overwrites_an_existing_report() {
        let directory = TestDirectory::new();
        let path = directory.0.join("report.json");
        fs::write(&path, "previous report").unwrap();
        assert!(write_probe_report(&directory.0, "report.json", &json!({"new": true})).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "previous report");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn successful_publication_is_complete_and_has_no_temporary_name() {
        let directory = TestDirectory::new();
        let expected = json!({"origin": "FIXTURE", "deliverable": false});
        write_probe_report(&directory.0, "report.json", &expected).unwrap();
        let contents = fs::read_to_string(directory.0.join("report.json")).unwrap();
        assert!(contents.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&contents).unwrap(),
            expected
        );
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn publication_rejects_non_basename_paths() {
        let directory = TestDirectory::new();
        for name in [
            "",
            ".",
            "..",
            "../report.json",
            "/report.json",
            "nested/report.json",
        ] {
            assert!(write_probe_report(&directory.0, name, &json!({})).is_err());
        }
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}
