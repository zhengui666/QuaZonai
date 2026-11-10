"""Validate local source paths used by the archive operator."""


def checked_path(path):
    if (not isinstance(path, str) or not path or "\\" in path
            or any(ord(char) < 32 or ord(char) == 127 for char in path)
            or any(part in ("", ".", "..") for part in path.split("/"))):
        raise ValueError("unsafe repository path")
    return path


def safe_local(root, relative):
    target = root / checked_path(relative)
    for path in (*target.parents, target):
        if path.is_symlink():
            raise ValueError("symlinks are not allowed in source paths")
    return target
