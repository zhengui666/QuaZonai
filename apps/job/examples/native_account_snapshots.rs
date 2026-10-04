//! Test-only official Portfolio::build_snapshot -> official serde NDJSON.
//! Controlled cash account; no LiveNode, Sandbox, fills, credentials or network.
#[path = "../tests/support/native_portfolio.rs"]
mod native_portfolio;

use std::io::Write;

fn main() -> anyhow::Result<()> {
    let mut native = native_portfolio::CashPortfolio::new();
    let mut output = std::io::stdout().lock();
    for index in 0..2 {
        if index == 1 {
            native.advance_account();
        }
        let snapshot = native
            .portfolio
            .build_snapshot(&native.account_id)
            .ok_or_else(|| anyhow::anyhow!("controlled native account has no snapshot"))?;
        serde_json::to_writer(&mut output, &snapshot)?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}
