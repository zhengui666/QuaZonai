from __future__ import annotations

from pathlib import Path

path = Path("backend/src/quant_runtime/promotion.py")
content = path.read_text(encoding="utf-8")
old = '''    raw = (candidate.metrics or {}).get("search_adjusted_quality")
    try:
        value = float(raw)
'''
new = '''    raw = (candidate.metrics or {}).get("search_adjusted_quality")
    if raw is None:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_MISSING",
            "Current Candidate lacks a numeric search-adjusted quality baseline.",
            422,
        )
    try:
        value = float(raw)  # type: ignore[arg-type]
'''
if content.count(old) != 1:
    raise RuntimeError(f"expected one candidate-quality block, found {content.count(old)}")
path.write_text(content.replace(old, new, 1), encoding="utf-8")
