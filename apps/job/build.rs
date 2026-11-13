use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=QZ_NATIVE_BUILD_LOCK");
    let lock = env::var_os("QZ_NATIVE_BUILD_LOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../Cargo.lock")
        });
    assert!(lock.is_absolute(), "native build lock must be absolute");
    println!("cargo:rerun-if-changed={}", lock.display());
    let document: toml::Value =
        toml::from_str(&fs::read_to_string(lock).unwrap()).expect("invalid Cargo.lock");
    let packages = document["package"].as_array().unwrap();
    let mut identity = None;
    for package in packages {
        if !package["name"].as_str().unwrap().starts_with("nautilus-") {
            continue;
        }
        let source = package["source"].as_str().expect(
            "path-only Nautilus dependency provenance is unsupported; preserve user sources",
        );
        let revision = if source
            .starts_with("git+https://github.com/nautechsystems/nautilus_trader.git")
        {
            source
                .rsplit_once('#')
                .expect("resolved official git revision required")
                .1
        } else if source == "registry+https://github.com/rust-lang/crates.io-index"
            || source == "registry+sparse+https://index.crates.io/"
        {
            ""
        } else {
            panic!("unsupported Nautilus source; preserve the user environment and report it instead of substituting dependencies");
        };
        let value = (
            package["version"].as_str().unwrap().to_owned(),
            revision.to_owned(),
            source.to_owned(),
        );
        if let Some(expected) = &identity {
            assert_eq!(expected, &value, "mixed Nautilus sources");
        }
        identity = Some(value);
    }
    let (version, revision, source) = identity.expect("missing Nautilus packages");
    println!("cargo:rustc-env=QZ_NAUTILUS_SOURCE={source}");
    println!("cargo:rustc-env=QZ_NAUTILUS_VERSION={version}");
    println!("cargo:rustc-env=QZ_NAUTILUS_REVISION={revision}");
}
