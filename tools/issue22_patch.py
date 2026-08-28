from pathlib import Path

path = Path("backend/src/db/domain_models.py")
text = path.read_text(encoding="utf-8")
old = "from datetime import datetime\n"
new = "from datetime import UTC, datetime\n"
if old not in text:
    raise SystemExit("domain_models datetime import pattern missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
print("UTC import fixed")
