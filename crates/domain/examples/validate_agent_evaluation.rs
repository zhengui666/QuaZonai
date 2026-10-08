//! Offline report validation only. Does not invoke a model, upload, or read credentials.
use std::{fs::File, io::Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: validate_agent_evaluation REPORT.json")?;
    if args.next().is_some() {
        return Err("expected exactly one report path".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .read_to_end(&mut bytes)?;
    let report = domain::agent_evaluation::parse(&bytes)?;
    println!("Valid report contract: {:?}; mode {:?}; {} cases. Uploaded runner evidence is not scientific qualification.", report.status, report.mode, report.cases.len());
    Ok(())
}
