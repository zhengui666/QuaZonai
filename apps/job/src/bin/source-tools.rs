//! Installed operator dispatcher. Legacy Cargo binaries use the same implementations.
use std::ffi::OsStr;

fn main() {
    let mut argv = std::env::args_os().skip(1);
    let Some(command) = argv.next() else {
        eprintln!("source-tools: expected catalog-prepare or polymarket-history");
        std::process::exit(2);
    };
    let run = match command.as_os_str() {
        value if value == OsStr::new("catalog-prepare") => job::operator::catalog_prepare::run,
        value if value == OsStr::new("polymarket-history") => {
            job::operator::polymarket_history::run
        }
        _ => {
            eprintln!("source-tools: expected catalog-prepare or polymarket-history");
            std::process::exit(2);
        }
    };
    // Preserve the selected command's argv[0], parser, diagnostics and exit behavior.
    run(std::iter::once(command).chain(argv).collect());
}
