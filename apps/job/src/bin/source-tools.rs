//! Installed operator dispatcher. Legacy Cargo binaries use the same implementations.
use std::ffi::{OsStr, OsString};

fn main() {
    let mut argv = std::env::args_os().skip(1);
    let Some(command) = argv.next() else {
        usage();
        std::process::exit(2);
    };
    let Some(run) = operation(command.as_os_str()) else {
        usage();
        std::process::exit(2);
    };
    // Preserve the selected command's argv[0], parser, diagnostics and exit behavior.
    run(std::iter::once(command).chain(argv).collect());
}

fn operation(command: &OsStr) -> Option<fn(Vec<OsString>)> {
    match command {
        value if value == OsStr::new("catalog-prepare") => Some(job::operator::catalog_prepare::run),
        value if value == OsStr::new("polymarket-history") => Some(job::operator::polymarket_history::run),
        _ => None,
    }
}

fn usage() {
    eprintln!("source-tools: expected catalog-prepare or polymarket-history");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_keeps_existing_imports_and_rejects_hyper_collection() {
        assert!(operation(OsStr::new("catalog-prepare")).is_some());
        assert!(operation(OsStr::new("polymarket-history")).is_some());
        assert!(operation(OsStr::new("hyperliquid-public")).is_none());
        assert!(operation(OsStr::new("capture-spot-candles")).is_none());
    }
}
