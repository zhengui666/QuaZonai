//! Native private roots and single-component object access; no client-selected host paths.
use crate::{Failure, Result};
use rustix::fs::{flock, openat, FlockOperation, Mode, OFlags};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

pub struct RuntimeRoot {
    pub path: PathBuf,
    // Retain the OS lock until the sole execution supervisor has stopped.
    _lock: File,
}

pub fn canonical_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(Failure::Invalid("absolute_directory"));
    }
    let metadata = fs::symlink_metadata(path)?;
    let canonical = fs::canonicalize(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || canonical != path {
        return Err(Failure::Invalid("native_directory"));
    }
    Ok(canonical)
}

pub fn private_directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => File::open(path.parent().ok_or(Failure::Integrity)?)?.sync_all()?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    canonical_directory(path)?;
    if fs::metadata(path)?.mode() & 0o077 != 0 {
        return Err(Failure::Invalid("private_directory_mode"));
    }
    Ok(())
}

impl RuntimeRoot {
    pub fn open(path: &Path) -> Result<Self> {
        private_directory(path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(OFlags::NOFOLLOW.bits() as i32)
            .open(path.join("executor.lock"))?;
        if !lock.metadata()?.is_file()
            || lock.metadata()?.nlink() != 1
            || lock.metadata()?.mode() & 0o077 != 0
        {
            return Err(Failure::Invalid("executor_lock"));
        }
        flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(|_| Failure::Busy)?;
        for child in ["jobs", "staging"] {
            private_directory(&path.join(child))?;
        }
        Ok(Self {
            path: path.to_owned(),
            _lock: lock,
        })
    }
    pub fn job(&self, run: contracts::Id, attempt: u32) -> PathBuf {
        self.path.join("jobs").join(format!("{run}-{attempt}"))
    }
}

pub fn read_file(path: &Path, maximum: usize, private: bool) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    bounded(file, maximum, private)
}
fn bounded(file: File, maximum: usize, private: bool) -> Result<Vec<u8>> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.len() == 0
        || metadata.len() > maximum as u64
        || (private && metadata.mode() & 0o077 != 0)
    {
        return Err(Failure::Invalid("native_file"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() != metadata.len() as usize {
        return Err(Failure::Integrity);
    }
    Ok(bytes)
}
pub fn directory_handle(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::DIRECTORY | OFlags::NOFOLLOW).bits() as i32)
        .open(path)?)
}
pub fn read_child(directory: &File, name: &str, maximum: usize) -> Result<Vec<u8>> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(Failure::Invalid("object_name"));
    }
    let descriptor = openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    bounded(File::from(descriptor), maximum, false)
}
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o444)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(0o444))?;
    file.sync_all()?;
    Ok(())
}

pub fn publish_directory(staging: &Path, destination: &Path) -> Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staging,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)?;
    File::open(destination.parent().ok_or(Failure::Integrity)?)?.sync_all()?;
    Ok(())
}

/// Only a job's own writable bind mount is inspected. This does not follow children.
pub fn output_usage(root: &Path, maximum: u64) -> Result<u64> {
    let directory = directory_handle(root)?;
    let _metadata = directory.metadata()?;
    let mut total = 0u64;
    let mut entries = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        entries += 1;
        let metadata = fs::symlink_metadata(entry.path())?;
        if entries > 128 || !metadata.is_file() || metadata.nlink() != 1 {
            return Err(Failure::Invalid("native_output_entry"));
        }
        total = total.checked_add(metadata.len()).ok_or(Failure::Capacity)?;
        if total > maximum {
            return Err(Failure::Capacity);
        }
    }
    Ok(total)
}

pub fn readonly_directory(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))?;
    File::open(path)?.sync_all()?;
    Ok(())
}

pub fn writable_output(path: &Path) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    // The private parent is never mounted. Only the isolated uid gets this bind mount.
    fs::set_permissions(path, fs::Permissions::from_mode(0o1777))?;
    File::open(path)?.sync_all()?;
    Ok(())
}
