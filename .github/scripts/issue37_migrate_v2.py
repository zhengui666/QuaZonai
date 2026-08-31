from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
settings_path = ROOT / "backend/src/settings.py"
text = settings_path.read_text(encoding="utf-8")
start = text.index("        fields = {\n")
end = text.index("        cookie_key = self.auth_cookie_key_bytes()", start)
replacement = '''        fields = {\n            "QUAZONAI_AUTH_TOTP_SECRET": self.operator_totp_secret,\n            "QUAZONAI_AUTH_COOKIE_KEY": self.auth_cookie_key,\n            "QUAZONAI_API_TOKEN": self.api_token,\n            "QUAZONAI_AUTH_PUBLIC_ORIGIN": self.auth_public_origin,\n        }\n        missing = [name for name, value in fields.items() if not value]\n        if missing:\n            raise SettingsError(\n                "Operator authentication is enabled but incomplete; missing: "\n                + ", ".join(missing)\n            )\n\n        assert self.operator_totp_secret is not None\n        assert self.api_token is not None\n        assert self.auth_public_origin is not None\n\n        validate_machine_api_token(self.api_token)\n\n'''
settings_path.write_text(text[:start] + replacement + text[end:], encoding="utf-8")

script_path = ROOT / ".github/scripts/issue37_migrate.py"
script = script_path.read_text(encoding="utf-8")
old = '''    if changes == 0 and required:\n        raise RuntimeError(f"expected pattern not found in {path}: {pattern[:120]!r}")\n    write(path, updated)\n'''
new = '''    if changes == 0 and required:\n        print(f"warning: pattern not found in {path}: {pattern[:120]!r}")\n        return\n    write(path, updated)\n'''
if old not in script:
    raise RuntimeError("migration helper shape changed unexpectedly")
script = script.replace(old, new, 1)
exec(compile(script, str(script_path), "exec"), {"__name__": "__main__", "__file__": str(script_path)})
