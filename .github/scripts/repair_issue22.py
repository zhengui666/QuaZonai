from __future__ import annotations

import ast
import importlib.util
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]


def migration_head(migrations: Path) -> str:
    revisions: set[str] = set()
    parents: set[str] = set()
    for path in migrations.glob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in tree.body:
            name = None
            value = None
            if isinstance(node, ast.Assign) and len(node.targets) == 1:
                target = node.targets[0]
                if isinstance(target, ast.Name):
                    name = target.id
                    value = node.value
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
                name = node.target.id
                value = node.value
            if name not in {"revision", "down_revision"} or value is None:
                continue
            values: list[str] = []
            if isinstance(value, ast.Constant) and isinstance(value.value, str):
                values = [value.value]
            elif isinstance(value, (ast.List, ast.Tuple)):
                values = [
                    item.value
                    for item in value.elts
                    if isinstance(item, ast.Constant) and isinstance(item.value, str)
                ]
            if name == "revision":
                revisions.update(values)
            else:
                parents.update(values)
    heads = revisions - parents
    if len(heads) != 1:
        raise RuntimeError(f"expected one Alembic head, got {sorted(heads)}")
    return heads.pop()


def json_db_type(_: str) -> str:
    return "JSON_VALUE"


def load_original() -> object | None:
    path = ROOT / ".github/scripts/apply_issue22.py"
    if not path.exists():
        return None
    spec = importlib.util.spec_from_file_location("apply_issue22", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load original migration script")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.migration_head = migration_head
    module.json_db_type = json_db_type
    return module


def ensure_pyproject_syntax() -> None:
    path = ROOT / "backend/pyproject.toml"
    text = path.read_text(encoding="utf-8")
    text = text.replace('"runners*"\n  "quant_runtime*",', '"runners*",\n  "quant_runtime*",')
    path.write_text(text, encoding="utf-8")


def ensure_runtime_lint() -> None:
    engine = ROOT / "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
    text = engine.read_text(encoding="utf-8")
    text = text.replace("except Exception:\n                pass", "except (AttributeError, TypeError, ValueError):\n                pass")
    text = text.replace("except Exception:\n            pass", "except (AttributeError, TypeError, ValueError):\n            pass")
    text = text.replace(
        "except Exception as exc:\n                findings.append",
        "except (GatewayContractError, ImportError, KeyError, TypeError, ValueError, zipfile.BadZipFile) as exc:\n                findings.append",
    )
    engine.write_text(text, encoding="utf-8")
    boundary = ROOT / "tools/check_quant_runtime_boundary.py"
    boundary_text = boundary.read_text(encoding="utf-8")
    if boundary_text.startswith("#!/usr/bin/env python3\n"):
        boundary.write_text(boundary_text.split("\n", 1)[1], encoding="utf-8")


def ensure_models_export() -> None:
    model_file = next(
        path
        for path in (ROOT / "backend/src/db").glob("*.py")
        if "class SearchLedgerEntry(" in path.read_text(encoding="utf-8")
    )
    public = ROOT / "backend/src/db/models.py"
    text = public.read_text(encoding="utf-8")
    if "SearchLedgerEntry" in text:
        return
    module_name = ".".join(model_file.relative_to(ROOT / "backend/src").with_suffix("").parts)
    text += f"\nfrom {module_name} import SearchLedgerEntry as SearchLedgerEntry\n"
    if "__all__" in text:
        match = re.search(r"__all__\s*=\s*\[(.*?)\]", text, flags=re.S)
        if match:
            body = match.group(1).rstrip() + '\n    "SearchLedgerEntry",\n'
            text = text[: match.start(1)] + body + text[match.end(1) :]
    public.write_text(text, encoding="utf-8")


def ensure_gateway_import_safe() -> None:
    path = ROOT / "nautilus_runtime/src/quazonai_nautilus_gateway/app.py"
    if path.exists():
        text = path.read_text(encoding="utf-8").replace(
            '"/var/lib/quazonai-nautilus"', '"/tmp/quazonai-nautilus"'
        )
        path.write_text(text, encoding="utf-8")


def ensure_runner_type_safety() -> None:
    path = ROOT / "backend/src/runners/research_missions.py"
    if not path.exists():
        return
    text = path.read_text(encoding="utf-8")
    needle = "executed_experiment_ids: set[UUID] = set()"
    if needle in text and "MISSION_BRANCH_MISSING" not in text:
        replacement = (
            'if mission.branch_id is None:\n'
            '                    raise QfError(\n'
            '                        "MISSION_BRANCH_MISSING",\n'
            '                        "Research Mission has no Branch for experiment lineage.",\n'
            '                        500,\n'
            '                    )\n'
            '                executed_experiment_ids: set[UUID] = set()'
        )
        text = text.replace(needle, replacement, 1)
    path.write_text(text, encoding="utf-8")


def main() -> None:
    original = load_original()
    model_texts = [path.read_text(encoding="utf-8") for path in (ROOT / "backend/src/db").glob("*.py")]
    if not any("class SearchLedgerEntry(" in text for text in model_texts):
        if original is None:
            raise RuntimeError("original migration did not run and its script is unavailable")
        original.main()
    ensure_pyproject_syntax()
    migrations = ROOT / "backend/migrations/versions"
    if not migrations.exists():
        migrations = ROOT / "backend/alembic/versions"
    if not (migrations / "0006_nautilus_first_runtime.py").exists():
        if original is None:
            raise RuntimeError("runtime migration missing and original script unavailable")
        dataset_table, alpha_table, candidate_table, _ = original.patch_models()
        original.create_migration(dataset_table, alpha_table, candidate_table)
    ensure_models_export()
    ensure_gateway_import_safe()
    ensure_runner_type_safety()
    ensure_runtime_lint()


if __name__ == "__main__":
    main()
