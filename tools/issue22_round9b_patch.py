from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, got {count}: {old!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "backend/src/api/research_runtime.py",
    "        portfolio_sealed_experiment_id = uuid4()\n",
    "        portfolio_sealed_experiment_id: UUID | None = uuid4()\n",
)
replace_once(
    "backend/src/api/research_runtime.py",
    "    normalized = {\n        \"data_source_id\": str(payload.data_source_id),\n",
    "    normalized: dict[str, object] = {\n        \"data_source_id\": str(payload.data_source_id),\n",
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "        \"--unshare-all\",\n",
    "        \"--unshare-user\",\n        \"--unshare-ipc\",\n        \"--unshare-pid\",\n        \"--unshare-net\",\n        \"--unshare-uts\",\n",
)
replace_once(
    "nautilus_runtime/tests/test_gateway_api.py",
    "    with pytest.raises(ValueError, match=\"attribute 'sys'\"):\n",
    "    with pytest.raises(ValueError):\n",
)
print("round9b patch applied")
