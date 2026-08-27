from __future__ import annotations

from pathlib import Path

path = Path("backend/src/quant_runtime/promotion.py")
content = path.read_text(encoding="utf-8")
old = "        value = float(raw)  # type: ignore[arg-type]\n"
new = "        value = float(raw)\n"
if content.count(old) != 1:
    raise RuntimeError(f"expected one redundant mypy suppression, found {content.count(old)}")
path.write_text(content.replace(old, new, 1), encoding="utf-8")
