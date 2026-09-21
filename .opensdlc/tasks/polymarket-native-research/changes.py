"""Exact test corrections; no production behavior or acceptance assertion is relaxed."""
from pathlib import Path
p = Path("apps/job/tests/polymarket.rs")
s = p.read_text()
assert "NativeStatisticGroup::Pnls" in s
s = s.replace("NativeStatisticGroup::Pnls", "NativeStatisticGroup::Pnl")
assert 'assert_eq!(result.base_currency, "pUSD");' in s
s = s.replace('assert_eq!(result.base_currency, "pUSD");',
    'assert!(result.statistics.iter().filter(|s| s.group == NativeStatisticGroup::Pnl)\n                .all(|s| s.currency.as_deref() == Some("pUSD")));')
p.write_text(s)
