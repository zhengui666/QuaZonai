use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path};

#[derive(Default)]
pub struct Budget;
impl Budget {
    pub fn read(&mut self, path: &Path) -> Result<Vec<u8>> {
        read(path)
    }
    pub fn relative(&mut self, root: &Path, path: &str) -> Result<Vec<u8>> {
        safe_relative(path)?;
        self.read(&root.join(path))
    }
}

fn safe_relative(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 512
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("unsafe relative input path");
    }
    Ok(())
}

/// Reject existing symlinks in every component, including caller-supplied roots.
/// This is a guard for operator-owned files, not a hostile concurrent-FS sandbox.
fn no_symlinks(path: &Path) -> Result<()> {
    let mut current = std::path::PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err("unsafe input path");
        }
        current.push(part);
        if fs::symlink_metadata(&current)
            .map_err(|_| "missing input path")?
            .file_type()
            .is_symlink()
        {
            return Err("symlink input is unsupported");
        }
    }
    Ok(())
}

pub fn directory(path: &Path) -> Result<()> {
    no_symlinks(path)?;
    if !fs::symlink_metadata(path)
        .map_err(|_| "missing input directory")?
        .is_dir()
    {
        return Err("input must be a directory");
    }
    Ok(())
}

pub fn create_directory(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    directory(parent)?;
    let mut options = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        options.mode(0o700);
    }
    options
        .create(path)
        .map_err(|_| "output directory already exists or cannot be created")
}

pub type Result<T> = std::result::Result<T, &'static str>;

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Reject duplicate keys before conversion into typed structs or JSON values.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut input: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut value = Map::new();
                while let Some((key, item)) = input.next_entry::<String, Unique>()? {
                    if value.insert(key, item.0).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Unique(Value::Object(value)))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut input: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut value = Vec::new();
                while let Some(item) = input.next_element::<Unique>()? {
                    value.push(item.0);
                }
                Ok(Unique(Value::Array(value)))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<Unique, E> {
                Ok(Unique(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, _value: f64) -> std::result::Result<Unique, E> {
                // Fractions, exponent notation and overflowing integer literals can
                // round during serde_json parsing. Never let that produce a PASS.
                Err(E::custom(
                    "floating-point JSON numbers require exact strings",
                ))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(value.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Unique, E> {
                Ok(Unique(value.into()))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
        }
        deserializer.deserialize_any(V)
    }
}

pub fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.is_empty() {
        return Err("JSON size limit");
    }
    let value =
        serde_json::from_slice::<Unique>(bytes).map_err(|_| "invalid or duplicate-key JSON")?;
    serde_json::from_value(value.0).map_err(|_| "JSON contract")
}

pub fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| "JSON serialization")?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    no_symlinks(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| "missing input file")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("input must be a regular file");
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "input open")?
        .read_to_end(&mut bytes)
        .map_err(|_| "input read")?;
    if bytes.is_empty() {
        return Err("input size limit");
    }
    Ok(bytes)
}

pub fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "output already exists or cannot be created")?;
    file.write_all(bytes).map_err(|_| "output write")?;
    file.sync_all().map_err(|_| "output sync")
}
