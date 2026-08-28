from __future__ import annotations

from pathlib import Path

path = Path("tools/issue22_round9_patch.py")
text = path.read_text(encoding="utf-8")
old = '''replace_once(
    ".github/workflows/ci.yml",
    '      - name: Install pinned remote runtime\\n'
    '        run: python -m pip install -e \\'nautilus_runtime[dev]\\'\\n',
    '      - name: Install OS capability sandbox\\n'
    '        run: |\\n'
    '          sudo apt-get update\\n'
    '          sudo apt-get install -y bubblewrap\\n'
    '          sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 || true\\n'
    '          bwrap --unshare-all --ro-bind /usr /usr --ro-bind /lib /lib --ro-bind /lib64 /lib64 --proc /proc --dev /dev /usr/bin/true\\n'
    '      - name: Install pinned remote runtime\\n'
    '        run: python -m pip install -e \\'nautilus_runtime[dev]\\'\\n',
)
'''
count = text.count(old)
if count != 1:
    raise RuntimeError(f"expected one round9 CI patch block, found {count}")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
