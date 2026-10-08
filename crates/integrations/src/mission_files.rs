//! Read-only native directory capabilities for one launcher-selected Mission.
//! No ambient file API is exposed to tools. Publication uses the existing HTTP
//! Artifact service; mutable worktree bytes are not immutable evidence by themselves.
use std::{fs, io::Read, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("mission file is unavailable or violates the workspace boundary")]
pub struct MissionFileError;

pub struct MissionFiles {
    root: fs::File,
}

fn components(value: &str) -> Result<Vec<&str>, MissionFileError> {
    if value.is_empty() || value.contains('\\') || value.chars().any(char::is_control) {
        return Err(MissionFileError);
    }
    let parts: Vec<_> = value.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return Err(MissionFileError);
    }
    Ok(parts)
}

impl MissionFiles {
    /// The trusted launcher chooses an absolute worktree root before granting any
    /// model tool access. This is not a path obtained from an MCP request.
    #[cfg(unix)]
    pub fn open(root: &Path) -> Result<Self, MissionFileError> {
        use std::os::unix::fs::OpenOptionsExt;
        if !root.is_absolute() {
            return Err(MissionFileError);
        }
        let opened = fs::OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK)
                    .bits() as i32,
            )
            .open(root)
            .map_err(|_| MissionFileError)?;
        if !opened.metadata().map_err(|_| MissionFileError)?.is_dir() {
            return Err(MissionFileError);
        }
        Ok(Self { root: opened })
    }

    #[cfg(not(unix))]
    pub fn open(_: &Path) -> Result<Self, MissionFileError> {
        Err(MissionFileError)
    }

    pub fn read_text(&self, relative: &str) -> Result<String, MissionFileError> {
        let bytes = self
            .read_bytes(relative, u64::MAX)
            .map_err(|_| MissionFileError)?;
        String::from_utf8(bytes).map_err(|_| MissionFileError)
    }

    /// Each component is opened against an already-held native directory handle.
    /// Links, special files, hidden metadata and traversal fail. Reads preserve the
    /// complete opened file and reject any concurrent change to its metadata.
    /// The deployment-side historical exporter also uses this for binary objects.
    #[cfg(unix)]
    pub fn read_bytes(&self, relative: &str, limit: u64) -> std::io::Result<Vec<u8>> {
        self.read_bounded(relative, limit)
    }

    #[cfg(unix)]
    fn read_bounded(&self, relative: &str, limit: u64) -> std::io::Result<Vec<u8>> {
        use rustix::fs::{openat, Mode, OFlags};
        use std::os::unix::fs::MetadataExt;
        let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
        if limit == 0 {
            return Err(invalid());
        }
        let parts = components(relative).map_err(|_| invalid())?;
        let mut directory = self.root.try_clone()?;
        // cap-std manages symlink following separately from custom flags. Native
        // openat receives one component and a held directory fd on every call.
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        for part in &parts[..parts.len() - 1] {
            let opened = openat(&directory, *part, flags | OFlags::DIRECTORY, Mode::empty())?;
            directory = fs::File::from(opened);
        }
        let opened = openat(&directory, parts[parts.len() - 1], flags, Mode::empty())?;
        let mut file = fs::File::from(opened);
        let before = file.metadata()?;
        if !before.is_file() || before.nlink() != 1 || before.len() == 0 || before.len() > limit {
            return Err(invalid());
        }
        let capacity = usize::try_from(before.len()).map_err(|_| invalid())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(|_| invalid())?;
        Read::by_ref(&mut file)
            .take(before.len().saturating_add(1))
            .read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if bytes.len() as u64 != before.len()
            || bytes.len() as u64 > limit
            || after.len() != before.len()
            || after.nlink() != 1
            || after.mtime() != before.mtime()
            || after.mtime_nsec() != before.mtime_nsec()
            || after.ctime() != before.ctime()
            || after.ctime_nsec() != before.ctime_nsec()
        {
            return Err(invalid());
        }
        Ok(bytes)
    }

    /// Deployment-only immutable copy. Native Linux seals freeze bytes even if
    /// the original export is later replaced. Each reader has its own position.
    #[cfg(target_os = "linux")]
    pub fn snapshot(&self, relative: &str, limit: u64) -> std::io::Result<FrozenFile> {
        use rustix::fs::{fcntl_add_seals, memfd_create, MemfdFlags, SealFlags};
        use std::io::Write;
        let bytes = self.read_bounded(relative, limit)?;
        let mut file = fs::File::from(memfd_create(
            "historical-export",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )?);
        file.write_all(&bytes)?;
        fcntl_add_seals(
            &file,
            SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
        )?;
        Ok(FrozenFile {
            file: std::sync::Arc::new(file),
            position: 0,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn snapshot(&self, _: &str, _: u64) -> std::io::Result<FrozenFile> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    #[cfg(not(unix))]
    pub fn read_bytes(&self, _: &str, _: u64) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

/// A sealed snapshot with an independent read cursor, never an exposed writer.
#[derive(Clone)]
pub struct FrozenFile {
    file: std::sync::Arc<fs::File>,
    position: u64,
}
impl std::io::Read for FrozenFile {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            let count = self.file.read_at(buffer, self.position)?;
            self.position += count as u64;
            Ok(count)
        }
        #[cfg(not(unix))]
        {
            let _ = buffer;
            Err(std::io::ErrorKind::Unsupported.into())
        }
    }
}
impl std::io::Seek for FrozenFile {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        let offset = match from {
            std::io::SeekFrom::Start(value) => i128::from(value),
            std::io::SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            std::io::SeekFrom::End(value) => {
                i128::from(self.file.metadata()?.len()) + i128::from(value)
            }
        };
        self.position = u64::try_from(offset)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        Ok(self.position)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[cfg(target_os = "linux")]
    #[test]
    fn snapshot_is_native_sealed_and_readers_are_independent() {
        use std::io::{Read, Seek, SeekFrom, Write};
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("rows.csv"), "original").unwrap();
        let mut first = MissionFiles::open(root.path())
            .unwrap()
            .snapshot("rows.csv", 512)
            .unwrap();
        let mut second = first.clone();
        fs::write(root.path().join("rows.csv"), "replacement").unwrap();
        assert!(first
            .file
            .try_clone()
            .unwrap()
            .write_all(b"changed")
            .is_err());
        assert!(first.file.set_len(0).is_err());
        let mut bytes = [0; 3];
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ori");
        let mut original = String::new();
        second.read_to_string(&mut original).unwrap();
        assert_eq!(original, "original");
        first.seek(SeekFrom::End(-3)).unwrap();
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"nal");
        assert!(first.seek(SeekFrom::Start(u64::MAX)).is_ok());
        assert!(first.seek(SeekFrom::Current(1)).is_err());
        assert!(second.seek(SeekFrom::Start(0)).is_ok());
        assert!(second.seek(SeekFrom::Current(-1)).is_err());
    }

    #[test]
    fn exact_utf8_bytes_are_read_from_the_opened_root_after_rename() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("work");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("reports")).unwrap();
        let content = "{\"schema_version\":1,\"结论\":\"未验证\"}\n";
        fs::write(root.join("reports/proposal.json"), content).unwrap();
        let files = MissionFiles::open(&root).unwrap();
        fs::rename(&root, parent.path().join("moved")).unwrap();
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("reports")).unwrap();
        fs::write(root.join("reports/proposal.json"), "replacement").unwrap();
        assert_eq!(files.read_text("reports/proposal.json").unwrap(), content);
    }

    #[test]
    fn paths_links_hardlinks_and_special_files_cannot_cross_the_boundary() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("work");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join("private"), "SECRET_SENTINEL").unwrap();
        fs::write(root.join("ordinary.rs"), "pub fn value() {}\n").unwrap();
        let files = MissionFiles::open(&root).unwrap();
        for bad in [
            "",
            "/etc/passwd",
            "../private",
            "a/../../private",
            "a//b",
            "./ordinary.rs",
            ".git/config",
            "a/.env",
            "a\\b",
            "a\0b",
        ] {
            assert!(files.read_text(bad).is_err(), "{bad:?}");
        }
        symlink(parent.path().join("private"), root.join("linked.rs")).unwrap();
        symlink(parent.path(), root.join("linked-directory")).unwrap();
        fs::hard_link(parent.path().join("private"), root.join("hardlinked.rs")).unwrap();
        assert!(files.read_text("linked.rs").is_err());
        assert!(files.read_text("linked-directory/private").is_err());
        assert!(files.read_text("hardlinked.rs").is_err());
        assert!(files.read_text("linked-directory").is_err());
        symlink(&root, parent.path().join("linked-root")).unwrap();
        assert!(MissionFiles::open(&parent.path().join("linked-root")).is_err());
        rustix::fs::mknodat(
            rustix::fs::CWD,
            root.join("pipe"),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        assert!(files.read_text("pipe").is_err());
        assert!(files.read_text("ordinary.rs").is_ok());
    }

    #[test]
    fn same_root_symlinks_are_rejected_not_only_escape_links() {
        let parent = tempfile::tempdir().unwrap();
        fs::create_dir(parent.path().join("real")).unwrap();
        fs::write(parent.path().join("real/code.rs"), "pub fn forecast() {}\n").unwrap();
        symlink("real/code.rs", parent.path().join("alias.rs")).unwrap();
        symlink("real", parent.path().join("alias-dir")).unwrap();
        let files = MissionFiles::open(parent.path()).unwrap();
        assert!(files.read_text("real/code.rs").is_ok());
        assert!(files.read_text("alias.rs").is_err());
        assert!(files.read_text("alias-dir/code.rs").is_err());
    }

    #[test]
    fn empty_oversize_and_non_utf8_are_not_uploaded() {
        let parent = tempfile::tempdir().unwrap();
        let files = MissionFiles::open(parent.path()).unwrap();
        for (name, bytes) in [
            ("empty.rs", Vec::new()),
            ("binary.rs", vec![0xff, 0xfe]),
        ] {
            fs::write(parent.path().join(name), bytes).unwrap();
            assert!(files.read_text(name).is_err());
        }
        let content = "界".repeat(2 * 1024 * 1024 / 3 + 1);
        fs::write(parent.path().join("large.rs"), &content).unwrap();
        assert_eq!(files.read_text("large.rs").unwrap(), content);
        assert!(components(&"a/".repeat(33)).is_err());
    }

    #[test]
    fn native_relative_paths_cross_former_length_and_depth_caps() {
        let parent = tempfile::tempdir().unwrap();
        let files = MissionFiles::open(parent.path()).unwrap();
        let long_component = "d".repeat(180);
        let long = format!("{long_component}/{long_component}/{long_component}/report.json");
        let deep = format!("{}report.json", "d/".repeat(40));
        assert!(long.len() > 512);
        assert!(deep.split('/').count() > 32);
        for relative in [long, deep] {
            let path = parent.path().join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "complete original report").unwrap();
            assert_eq!(
                files.read_text(&relative).unwrap(),
                "complete original report"
            );

            // Long/deep valid prefixes do not weaken traversal or link checks.
            let prefix = relative.rsplit_once('/').unwrap().0;
            assert!(
                files
                    .read_text(&format!("{prefix}/../report.json"))
                    .is_err()
            );
            assert!(files.read_text(&format!("{prefix}/.hidden")).is_err());
            let link = format!("{prefix}/alias.json");
            symlink(&path, parent.path().join(&link)).unwrap();
            assert!(files.read_text(&link).is_err());
        }
    }
}
