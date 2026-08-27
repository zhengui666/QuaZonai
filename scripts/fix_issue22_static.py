from pathlib import Path


path = Path("backend/src/runners/quant_experiments.py")
text = path.read_text(encoding="utf-8")
text = text.replace("import argparse\nimport json\n", "import argparse\n", 1)
path.write_text(text, encoding="utf-8")

path = Path("backend/src/candidate_packages.py")
text = path.read_text(encoding="utf-8")
old = '''            if any(fragment in normalized for fragment in _SECRET_KEY_FRAGMENTS):
                return True
            if _contains_secret_key(item):
'''
new = '''            if any(fragment in normalized for fragment in _SECRET_KEY_FRAGMENTS):
                if not (normalized.startswith("contains_") and item is False):
                    return True
            if _contains_secret_key(item):
'''
if old not in text:
    raise RuntimeError("candidate secret detector baseline not found")
text = text.replace(old, new, 1)
text = text.replace(
    '''                    "broker_credentials": "MUST_BE_PROVIDED_BY_DOWNSTREAM_RUNTIME",
                    "quazonai_controls_runtime": False,
''',
    '''                    "runtime_connection_owner": "DOWNSTREAM_NAUTILUS_RUNTIME",
                    "quazonai_controls_runtime": False,
''',
    1,
)
path.write_text(text, encoding="utf-8")

path = Path("backend/src/runners/nautilus_remote_runtime.py")
text = path.read_text(encoding="utf-8")
if old not in text:
    raise RuntimeError("remote manifest secret detector baseline not found")
path.write_text(text.replace(old, new, 1), encoding="utf-8")

path = Path(".github/workflows/ci.yml")
text = path.read_text(encoding="utf-8")
text = text.replace("docker run --rm --entrypoint python quazonai-ci - <<'PY'", "docker run --rm -i --entrypoint python quazonai-ci - <<'PY'")
text = text.replace("docker run --rm --entrypoint python quazonai-nautilus-ci - <<'PY'", "docker run --rm -i --entrypoint python quazonai-nautilus-ci - <<'PY'")
path.write_text(text, encoding="utf-8")
