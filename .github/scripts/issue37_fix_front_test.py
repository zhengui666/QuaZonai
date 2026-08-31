from pathlib import Path

path = Path("frontend/src/auth/AuthGate.test.tsx")
text = path.read_text(encoding="utf-8")
old = '''    expect(JSON.parse(String(loginOptions.body))).toEqual({
      username: 'local-operator',
      password: 'correct horse battery staple',
      totp_code: '123456',
      trust_browser: true,
    });'''
new = '''    expect(JSON.parse(String(loginOptions.body))).toEqual({
      totp_code: '123456',
      trust_browser: true,
    });'''
count = text.count(old)
if count == 0:
    raise SystemExit("old login body assertion not found")
text = text.replace(old, new)
path.write_text(text, encoding="utf-8")
print(f"updated {count} login body assertion(s)")
