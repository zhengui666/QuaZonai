//! Native immutable local objects. Authorization and publication metadata stay in Store.
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use contracts::{DbCounter, Id};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use thiserror::Error;

pub const MAX_LOCAL_OBJECT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact storage operation failed")]
    Io(#[from] std::io::Error),
    #[error("artifact size, permissions or object type is invalid")]
    Invalid,
}

pub struct ArtifactStore {
    root: Dir,
}
impl ArtifactStore {
    /// The deployment owns the private parent directory; never chmod existing user files.
    pub fn open(path: &Path) -> Result<Self, ArtifactError> {
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                builder.mode(0o700);
                if let Err(error) = builder.create(path) {
                    if error.kind() != std::io::ErrorKind::AlreadyExists {
                        return Err(error.into());
                    }
                }
                fs::File::open(path.parent().ok_or(ArtifactError::Invalid)?)?.sync_all()?;
            }
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ArtifactError::Invalid);
        }
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(ArtifactError::Invalid);
        }
        Ok(Self {
            root: Dir::open_ambient_dir(path, ambient_authority())?,
        })
    }

    /// Publish exactly one object. A reported failure never authorizes DB publication.
    /// An uncertain directory sync retains the object, never deletes a possible reference.
    pub fn put(&self, id: Id, bytes: &[u8]) -> Result<(), ArtifactError> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_LOCAL_OBJECT_BYTES {
            return Err(ArtifactError::Invalid);
        }
        let pending = format!(".pending-{}", Id::new());
        let target = id.to_string();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = self.root.open_with(&pending, &options)?.into_std();
        let result = (|| -> Result<(), ArtifactError> {
            file.write_all(bytes)?;
            file.sync_all()?;
            #[cfg(unix)]
            file.set_permissions(fs::Permissions::from_mode(0o400))?;
            file.sync_all()?;
            self.root.hard_link(&pending, &self.root, &target)?;
            self.root.open(".")?.into_std().sync_all()?;
            Ok(())
        })();
        // Only our freshly created staging name is eligible, not the published target.
        let _ = self.root.remove_file(&pending);
        result
    }

    /// Trusted failed-publication recovery only. The caller must hold the original
    /// Store authority lock and have proved this native object is unreferenced.
    /// Never accepts a path, follows a link, scans a directory or removes a secret.
    pub fn discard_unpublished(&self, id: Id) -> Result<(), ArtifactError> {
        let name = id.to_string();
        let metadata = match self.root.symlink_metadata(&name) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ArtifactError::Invalid);
        }
        #[cfg(unix)]
        if cap_std::fs::PermissionsExt::mode(&metadata.permissions()) & 0o777 != 0o400 {
            return Err(ArtifactError::Invalid);
        }
        self.root.remove_file(&name)?;
        self.root.open(".")?.into_std().sync_all()?;
        Ok(())
    }

    pub fn read(&self, id: Id, expected_bytes: DbCounter) -> Result<Vec<u8>, ArtifactError> {
        let count = expected_bytes.get();
        if count == 0 || count > MAX_LOCAL_OBJECT_BYTES {
            return Err(ArtifactError::Invalid);
        }
        let name = id.to_string();
        let metadata = self.root.symlink_metadata(&name)?;
        if !metadata.is_file() || metadata.len() != count {
            return Err(ArtifactError::Invalid);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            );
        }
        let mut file = self.root.open_with(&name, &options)?.into_std();
        let opened = file.metadata()?;
        if !opened.is_file() || opened.len() != count {
            return Err(ArtifactError::Invalid);
        }
        #[cfg(unix)]
        if opened.permissions().mode() & 0o777 != 0o400 {
            return Err(ArtifactError::Invalid);
        }
        let mut bytes = Vec::with_capacity(count as usize);
        Read::by_ref(&mut file)
            .take(count + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != count || file.metadata()?.len() != count {
            return Err(ArtifactError::Invalid);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bytes_survive_reopen_and_collisions_never_overwrite() {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("artifacts");
        let store = ArtifactStore::open(&path).unwrap();
        let id = Id::new();
        store.put(id, b"original").unwrap();
        assert!(store.put(id, b"changed!").is_err());
        drop(store);
        let store = ArtifactStore::open(&path).unwrap();
        assert_eq!(
            store.read(id, DbCounter::new(8).unwrap()).unwrap(),
            b"original"
        );
        assert!(store.read(id, DbCounter::new(7).unwrap()).is_err());
        assert!(store.read(id, DbCounter::new(9).unwrap()).is_err());
        assert_eq!(fs::read_dir(path).unwrap().count(), 1);
    }

    #[test]
    fn trusted_discard_is_idempotent_and_never_scans_other_native_objects() {
        let parent = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(&parent.path().join("objects")).unwrap();
        let abandoned = Id::new();
        let retained = Id::new();
        store.put(abandoned, b"abandoned").unwrap();
        store.put(retained, b"retained").unwrap();
        store.discard_unpublished(abandoned).unwrap();
        store.discard_unpublished(abandoned).unwrap();
        assert_eq!(
            store.read(retained, DbCounter::new(8).unwrap()).unwrap(),
            b"retained"
        );
        assert!(!parent
            .path()
            .join("objects")
            .join(abandoned.to_string())
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn trusted_discard_refuses_links_and_nonpublished_permissions() {
        use std::os::unix::fs::symlink;
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("objects");
        let store = ArtifactStore::open(&root).unwrap();
        let original = Id::new();
        let link = Id::new();
        store.put(original, b"original").unwrap();
        symlink(root.join(original.to_string()), root.join(link.to_string())).unwrap();
        assert!(store.discard_unpublished(link).is_err());
        assert_eq!(
            store.read(original, DbCounter::new(8).unwrap()).unwrap(),
            b"original"
        );
        fs::set_permissions(
            root.join(original.to_string()),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(store.discard_unpublished(original).is_err());
        assert!(root.join(original.to_string()).is_file());
    }

    #[test]
    fn rejects_empty_or_unbounded_objects_before_allocation() {
        let parent = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(&parent.path().join("objects")).unwrap();
        assert!(store.put(Id::new(), b"").is_err());
        assert!(store.read(Id::new(), DbCounter::ZERO).is_err());
        assert!(store
            .read(
                Id::new(),
                DbCounter::new(MAX_LOCAL_OBJECT_BYTES + 1).unwrap()
            )
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_writable_objects_and_nonprivate_roots() {
        use std::os::unix::fs::symlink;
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("objects");
        let store = ArtifactStore::open(&path).unwrap();
        let id = Id::new();
        store.put(id, b"original").unwrap();
        let alias = Id::new();
        symlink(path.join(id.to_string()), path.join(alias.to_string())).unwrap();
        assert!(store.read(alias, DbCounter::new(8).unwrap()).is_err());
        fs::set_permissions(path.join(id.to_string()), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(store.read(id, DbCounter::new(8).unwrap()).is_err());
        let linked = parent.path().join("linked-root");
        symlink(&path, &linked).unwrap();
        assert!(ArtifactStore::open(&linked).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(ArtifactStore::open(&path).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
