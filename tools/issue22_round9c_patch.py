from pathlib import Path

path = Path("nautilus_runtime/src/quazonai_nautilus_gateway/engine.py")
text = path.read_text(encoding="utf-8")
old = "    gateway_source = Path(__file__).resolve().parents[2]\n"
new = "    gateway_source = Path(__file__).resolve().parents[1]\n"
if text.count(old) != 1:
    raise SystemExit("expected one gateway source-root match")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
print("sandbox package root fixed")
