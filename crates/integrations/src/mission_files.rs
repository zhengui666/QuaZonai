//! Read-only native directory capabilities for one launcher-selected Mission.
//! No ambient file API is exposed to tools. Publication uses the existing HTTP
//! Artifact service; mutable worktree bytes are not immutable evidence by themselves.
use contracts::artifacts::MAX_UPLOAD_BYTES;
use std::{fs, io::Read, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("mission file is unavailable or violates the workspace boundary")]
pub struct MissionFileError;

pub struct MissionFiles {
    root: fs::File,
}

fn components(value: &str) -> Result<Vec<&str>, MissionFileError> {
    if value.is_empty()
        || value.len() > 512
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(MissionFileError);
    }
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() > 32
        || parts
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

    /// Each component is opened against an already-held native directory handle.
    /// Links, special files, hidden metadata, traversal and unbounded reads fail.
    #[cfg(unix)]
    pub fn read_text(&self, relative: &str) -> Result<String, MissionFileError> {
        use rustix::fs::{openat, Mode, OFlags};
        use std::os::unix::fs::MetadataExt;
        let parts = components(relative)?;
        let mut directory = self.root.try_clone().map_err(|_| MissionFileError)?;
        // cap-std deliberately manages symlink following separately from custom
        // open flags. Use native openat for this stricter no-symlink policy;
        // every call receives exactly one component and a held directory fd.
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        for part in &parts[..parts.len() - 1] {
            let opened = openat(&directory, *part, flags | OFlags::DIRECTORY, Mode::empty())
                .map_err(|_| MissionFileError)?;
            directory = fs::File::from(opened);
        }
        let opened = openat(&directory, parts[parts.len() - 1], flags, Mode::empty())
            .map_err(|_| MissionFileError)?;
        let mut file = fs::File::from(opened);
        let before = file.metadata().map_err(|_| MissionFileError)?;
        if !before.is_file()
            || before.nlink() != 1
            || before.len() == 0
            || before.len() > MAX_UPLOAD_BYTES as u64
        {
            return Err(MissionFileError);
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        Read::by_ref(&mut file)
            .take(MAX_UPLOAD_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| MissionFileError)?;
        let after = file.metadata().map_err(|_| MissionFileError)?;
        if bytes.len() as u64 != before.len()
            || bytes.len() > MAX_UPLOAD_BYTES
            || after.len() != before.len()
            || after.nlink() != 1
            || after.mtime() != before.mtime()
            || after.mtime_nsec() != before.mtime_nsec()
            || after.ctime() != before.ctime()
            || after.ctime_nsec() != before.ctime_nsec()
        {
            return Err(MissionFileError);
        }
        String::from_utf8(bytes).map_err(|_| MissionFileError)
    }

    #[cfg(not(unix))]
    pub fn read_text(&self, _: &str) -> Result<String, MissionFileError> {
        Err(MissionFileError)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

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
            ("oversized.rs", vec![b'x'; MAX_UPLOAD_BYTES + 1]),
            ("binary.rs", vec![0xff, 0xfe]),
        ] {
            fs::write(parent.path().join(name), bytes).unwrap();
            assert!(files.read_text(name).is_err());
        }
        assert!(components(&"a/".repeat(33)).is_err());
    }
}
