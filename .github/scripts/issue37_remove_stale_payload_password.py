from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
path = ROOT / "backend/tests/integration/test_operator_auth_hardening.py"
text = path.read_text(encoding="utf-8")
stale = ', password="correct horse battery staple"'
count = text.count(stale)
if count == 0:
    raise SystemExit("no stale TOTP helper password arguments found")
text = text.replace(stale, "")
if "password=" in text:
    raise SystemExit("unexpected password keyword remains in TOTP-only hardening tests")
path.write_text(text, encoding="utf-8")
print(f"removed {count} stale password keyword argument(s)")
