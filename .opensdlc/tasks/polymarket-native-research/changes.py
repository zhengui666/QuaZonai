"""Apply only the explicitly authored textual changes on this development branch.
This temporary operation is removed after application; it does not generate business logic.
"""
from pathlib import Path

def replace(path: str, old: str, new: str, minimum: int = 1) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count < minimum:
        raise RuntimeError(f"Expected authored source context in {path}: {old!r}")
    file.write_text(text.replace(old, new))

replace("crates/contracts/src/lib.rs", "pub mod research;\n", "pub mod research;\npub mod research_currency;\n")
for path in ["brief.rs", "forward.rs", "portfolio.rs", "science.rs"]:
    replace(f"crates/contracts/src/{path}", "crate::budget::currency_schema", "crate::research_currency::schema")

for path, expression in [
    ("crates/domain/src/brief.rs", "&content.base_currency"),
    ("crates/domain/src/forward.rs", "&request.base_currency"),
    ("crates/domain/src/portfolio.rs", "&settings.base_currency"),
    ("crates/domain/src/portfolio.rs", "&input.base_currency"),
    ("crates/domain/src/portfolio.rs", "&content.base_currency"),
    ("crates/domain/src/execution/output.rs", "&target.currency"),
    ("crates/domain/src/execution/output/simulation.rs", "code"),
]:
    replace(path, f"iso_currency::Currency::from_code({expression}).is_none()",
            f"!contracts::research_currency::supported({expression})")
replace("crates/domain/src/brief.rs", '"ISO_CURRENCY_REQUIRED"', '"UNSUPPORTED_RESEARCH_CURRENCY"')

replace("DESIGN.md", "## Polymarket 原生历史数据准备\n", """## Polymarket 研究抵押币

研究的 base_currency 与模型账单 cost_currency 分开：前者接受原 ISO 4217
及锁定 Nautilus 的 USDC、USDC.e、pUSD，后者仍仅接受原 ISO 4217。
这些是不同资产的精确原生代码，不自动按 1:1 换算为 USD，也不能相互替换。
数据源与原生资产定义仍需证明实际 collateral contract、历史版本和可用时点。
资产货币、研究 Brief、执行假设、预测、组合、模拟结果与目标快照必须同币种。
数据库原生 research currency 列扩为 text，保留现有数据及模型账单限制。
接纳币种不是数据许可、PIT、费用或市场模拟能力通过，现有准入仍须逐项验证。

## Polymarket 原生历史数据准备
""")
