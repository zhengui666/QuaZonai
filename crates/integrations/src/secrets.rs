//! Operator-controlled encrypted secret files; no plaintext secret read API.
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use chacha20poly1305::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng, Payload},
    XChaCha20Poly1305, XNonce,
};
use contracts::Id;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use thiserror::Error;

const PREFIX: &[u8] = b"QZ-SECRET-V1\0";
const LIMIT: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("secret storage operation failed")]
    Io(#[from] std::io::Error),
    #[error("invalid secret or key material")]
    Invalid,
    #[error("secret authentication failed")]
    Authentication,
}

pub struct SecretVault {
    root: Dir,
    cipher: XChaCha20Poly1305,
}

impl SecretVault {
    /// Creates exactly one owner-readable native key; never overwrites a key.
    pub fn initialize_key(path: &Path) -> Result<(), SecretError> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(path)?;
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        file.write_all(&key)?;
        file.sync_all()?;
        fs::File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?
        .sync_all()?;
        Ok(())
    }

    pub fn open(root: &Path, key_path: &Path) -> Result<Self, SecretError> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let mut key_file = options.open(key_path)?;
        let metadata = key_file.metadata()?;
        if !metadata.is_file() || metadata.len() != 32 {
            return Err(SecretError::Invalid);
        }
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SecretError::Invalid);
        }
        let mut key = [0_u8; 32];
        key_file.read_exact(&mut key)?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        key.fill(0);
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() {
            return Err(SecretError::Invalid);
        }
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SecretError::Invalid);
        }
        Ok(Self {
            root: Dir::open_ambient_dir(root, ambient_authority())?,
            cipher,
        })
    }

    pub fn put(&self, purpose: &str, plaintext: &[u8]) -> Result<Id, SecretError> {
        if plaintext.is_empty() || plaintext.len() > LIMIT || !valid_purpose(purpose) {
            return Err(SecretError::Invalid);
        }
        let id = Id::new();
        let aad = format!("{id}:{purpose}");
        let mut nonce = [0_u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Authentication)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = self.root.open_with(id.to_string(), &options)?.into_std();
        let written = (|| -> Result<(), SecretError> {
            file.write_all(PREFIX)?;
            file.write_all(&nonce)?;
            file.write_all(&ciphertext)?;
            file.sync_all()?;
            #[cfg(unix)]
            file.set_permissions(fs::Permissions::from_mode(0o400))?;
            file.sync_all()?;
            self.root.open(".")?.into_std().sync_all()?;
            Ok(())
        })();
        if written.is_err() {
            // create_new already succeeded. Never remove an earlier/colliding
            // object, and never publish a reference to a partially written file.
            let _ = self.root.remove_file(id.to_string());
            let _ = self.root.open(".").and_then(|d| d.into_std().sync_all());
        }
        written.map(|()| id)
    }

    /// Trusted maintenance only; returned identities are not secret contents.
    /// Other purposes and unauthenticated/tampered objects are never candidates.
    pub fn machine_verifier_ids(&self) -> Result<Vec<Id>, SecretError> {
        let mut ids = Vec::new();
        for entry in self.root.entries()? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(id) = Id::try_from(name) else { continue };
            match self.read(id, "MACHINE_VERIFIER") {
                Ok(_) => ids.push(id),
                Err(SecretError::Authentication | SecretError::Invalid) => {}
                Err(SecretError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// The caller must hold the database publication barrier and prove this
    /// exact verifier is unreferenced. Never expose this via an Agent/HTTP API.
    pub fn remove_unpublished_verifier(&self, id: Id) -> Result<(), SecretError> {
        match self.read(id, "MACHINE_VERIFIER") {
            Ok(_) => {}
            Err(SecretError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
        self.root.remove_file(id.to_string())?;
        self.root.open(".")?.into_std().sync_all()?;
        Ok(())
    }

    /// Trusted process only. The UUID path and authenticated purpose prevent
    /// traversal and ciphertext substitution between different secret classes.
    pub fn read(&self, id: Id, purpose: &str) -> Result<Vec<u8>, SecretError> {
        if !valid_purpose(purpose) {
            return Err(SecretError::Invalid);
        }
        let metadata = self.root.symlink_metadata(id.to_string())?;
        if !metadata.is_file() || metadata.len() as usize > LIMIT + PREFIX.len() + 40 {
            return Err(SecretError::Invalid);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let mut file = self.root.open_with(id.to_string(), &options)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((LIMIT + 100) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() < PREFIX.len() + 24 + 16 || !bytes.starts_with(PREFIX) {
            return Err(SecretError::Invalid);
        }
        let nonce = XNonce::from_slice(&bytes[PREFIX.len()..PREFIX.len() + 24]);
        let aad = format!("{id}:{purpose}");
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &bytes[PREFIX.len() + 24..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Authentication)
    }
}

fn valid_purpose(purpose: &str) -> bool {
    matches!(
        purpose,
        "TOTP" | "RUNTIME" | "DOWNSTREAM" | "CUSTOM_PROVIDER" | "SESSION_KEY" | "MACHINE_VERIFIER"
    )
}
