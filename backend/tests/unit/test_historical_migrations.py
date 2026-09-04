"""Keep historical Alembic revisions independent from live ORM models."""

from __future__ import annotations

import ast
from pathlib import Path


_VERSIONS = Path(__file__).parents[2] / "alembic" / "versions"
_FROZEN_REVISIONS = (
    "0001_initial.py",
    "0002_research_intelligence_boundary.py",
    "0004_runtime_configuration.py",
    "0006_nautilus_remote_runtime.py",
    "0009_archive_manifests.py",
    "0010_operator_auth_configuration.py",
    "0011_codex_model_runtime_controls.py",
)


def test_frozen_migrations_do_not_depend_on_live_orm_models() -> None:
    for filename in _FROZEN_REVISIONS:
        tree = ast.parse((_VERSIONS / filename).read_text())
        imports = [
            node.module
            for node in ast.walk(tree)
            if isinstance(node, ast.ImportFrom) and node.module and node.module.startswith("db")
        ]
        imports.extend(
            alias.name
            for node in ast.walk(tree)
            if isinstance(node, ast.Import)
            for alias in node.names
            if alias.name.startswith("db")
        )
        assert not imports, f"{filename} imports live ORM modules: {imports}"
        assert not any(
            isinstance(node, ast.Attribute)
            and node.attr == "metadata"
            and isinstance(node.value, ast.Name)
            and node.value.id == "Base"
            for node in ast.walk(tree)
        ), f"{filename} uses Base.metadata"
        assert not any(
            isinstance(node, ast.Attribute) and node.attr == "__table__"
            for node in ast.walk(tree)
        ), f"{filename} uses a live ORM table"
