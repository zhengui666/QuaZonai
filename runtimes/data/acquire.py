"""Read original local source files without changing their contents."""

import os
from pathlib import Path
import stat

from snapshot import safe_local


ADMISSION = {"coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
             "research_qualified": False, "registered_in_quazonai": False,
             "permission_status": "REQUIRES_INDEPENDENT_REVIEW"}


def local_bytes(path):
    path = Path(os.path.abspath(path))
    safe_local(path.parent, path.name)
    if not path.is_file():
        raise ValueError("local input missing or not a regular file")
    flags = (os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
             | getattr(os, "O_BINARY", 0))
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode):
            raise ValueError("local input is not a regular file")
        body = stream.read()
        after = os.fstat(stream.fileno())
    safe_local(path.parent, path.name)
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if (len(body) != before.st_size
            or identity(before) != identity(after) or identity(after) != identity(path.stat())):
        raise ValueError("local input changed during read")
    return body
