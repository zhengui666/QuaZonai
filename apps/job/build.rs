use std::{env, fs, path::PathBuf};

fn main() {
    let lock = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let document: toml::Value =
        toml::from_str(&fs::read_to_string(lock).unwrap()).expect("invalid Cargo.lock");
    let packages = document["package"].as_array().unwrap();
    let mut identity = None;
    for package in packages {
        if !package["name"].as_str().unwrap().starts_with("nautilus-") {
            continue;
        }
        let source = package["source"]
            .as_str()
            .expect("Nautilus must use official git source");
        assert!(
            source.starts_with("git+https://github.com/nautechsystems/nautilus_trader.git?rev=")
        );
        let revision = source.rsplit_once('#').unwrap().1;
        let value = (
            package["version"].as_str().unwrap().to_owned(),
            revision.to_owned(),
        );
        if let Some(expected) = &identity {
            assert_eq!(expected, &value, "mixed Nautilus sources");
        }
        identity = Some(value);
    }
    let (version, revision) = identity.expect("missing Nautilus packages");
    println!("cargo:rustc-env=QZ_NAUTILUS_VERSION={version}");
    println!("cargo:rustc-env=QZ_NAUTILUS_REVISION={revision}");
}
