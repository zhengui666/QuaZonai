"""Exact authored source corrections and test invocation; removed after application."""
from pathlib import Path

def replace(path, old, new):
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise RuntimeError(f"Missing authored context in {path}: {old!r}")
    file.write_text(text.replace(old, new))

replace("crates/domain/src/prediction.rs",
        '.ok_or_else(|| DomainError::CapabilityUnavailable("polymarket_fee_schedule_missing"))?',
        '.ok_or(DomainError::CapabilityUnavailable("polymarket_fee_schedule_missing"))?')
replace("apps/job/tests/polymarket.rs", 'use contracts::{portfolio::*, science::*, SchemaV1};', 'use contracts::{portfolio::*, science::*};')
replace("apps/job/tests/polymarket.rs", 'use nautilus_model::{data::BarType, instruments::Instrument};\n', '')
replace("apps/job/tests/polymarket.rs", 'use std::{fs, path::Path, str::FromStr};', 'use std::{fs, path::Path};')
replace(".github/workflows/prepare-polymarket.yml",
        'cargo test --locked -p job --features polymarket-history --bin polymarket-history\n',
        'cargo test --locked -p job --features polymarket-history --bin polymarket-history\n          cargo test --locked -p job --test polymarket -- --nocapture\n')
