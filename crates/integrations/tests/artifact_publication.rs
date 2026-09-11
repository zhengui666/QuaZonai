//! Native filesystem publication races and storage-integrity boundaries.
use contracts::{DbCounter, Id};
use integrations::artifacts::{ArtifactError, ArtifactStore};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

fn count(bytes: &[u8]) -> DbCounter {
    DbCounter::new(bytes.len() as u64).unwrap()
}

#[test]
fn concurrent_publishers_have_one_complete_winner_and_leave_no_staging_names() {
    const WRITERS: usize = 8;
    const OBJECT_BYTES: usize = 32 * 1024;
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("artifacts");
    let store = Arc::new(ArtifactStore::open(&path).unwrap());
    let barrier = Arc::new(Barrier::new(WRITERS));
    let id = Id::new();
    let mut writers = Vec::with_capacity(WRITERS);
    for writer in 0..WRITERS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        writers.push(thread::spawn(move || {
            let bytes = vec![u8::try_from(writer + 1).unwrap(); OBJECT_BYTES];
            barrier.wait();
            match store.put(id, &bytes) {
                Ok(()) => Some(bytes),
                Err(ArtifactError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    None
                }
                Err(error) => panic!("unexpected publication error: {error}"),
            }
        }));
    }
    let winners: Vec<_> = writers
        .into_iter()
        .filter_map(|writer| writer.join().unwrap())
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "a native object identity has one publisher"
    );
    assert_eq!(store.read(id, count(&winners[0])).unwrap(), winners[0]);
    drop(store);
    let reopened = ArtifactStore::open(&path).unwrap();
    assert_eq!(reopened.read(id, count(&winners[0])).unwrap(), winners[0]);
    let names: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from(id.to_string())]);
}

#[cfg(unix)]
#[test]
fn an_external_symlink_cannot_be_read_or_replaced_by_publication() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("artifacts");
    let store = ArtifactStore::open(&path).unwrap();
    let victim = parent.path().join("unrelated-user-data");
    let original = b"existing user bytes";
    fs::write(&victim, original).unwrap();
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o400)).unwrap();
    let id = Id::new();
    let target = path.join(id.to_string());
    symlink(&victim, &target).unwrap();

    assert!(store.read(id, count(original)).is_err());
    assert!(matches!(
        store.put(id, b"replacement bytes"),
        Err(ArtifactError::Io(error))
            if error.kind() == std::io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&victim).unwrap(), original);
    assert!(fs::symlink_metadata(&target)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_dir(path).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn object_length_drift_is_rejected_even_after_read_only_mode_is_restored() {
    use std::os::unix::fs::PermissionsExt;

    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("artifacts");
    let store = ArtifactStore::open(&path).unwrap();
    let id = Id::new();
    let original = b"original object bytes";
    store.put(id, original).unwrap();
    let object = path.join(id.to_string());
    // Local fault injection represents damaged storage, not an authorized API.
    // It must never silently change the immutable DB byte_count contract.
    for changed in [&b"short"[..], &b"longer than the original object bytes"[..]] {
        fs::set_permissions(&object, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&object, changed).unwrap();
        fs::set_permissions(&object, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(matches!(
            store.read(id, count(original)),
            Err(ArtifactError::Invalid)
        ));
    }
}

#[test]
fn directory_collisions_do_not_remove_existing_children_or_publish_partial_bytes() {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("artifacts");
    let store = ArtifactStore::open(&path).unwrap();
    let id = Id::new();
    let occupied = path.join(id.to_string());
    fs::create_dir(&occupied).unwrap();
    let child = occupied.join("existing-data");
    fs::write(&child, b"preserve").unwrap();

    assert!(store.put(id, b"new source").is_err());
    assert!(store.read(id, DbCounter::new(10).unwrap()).is_err());
    assert_eq!(fs::read(child).unwrap(), b"preserve");
    let names: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from(id.to_string())]);
}
