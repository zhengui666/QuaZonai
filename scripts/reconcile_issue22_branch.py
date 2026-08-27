from __future__ import annotations

import runpy
from pathlib import Path


def run_if_present(path: str) -> None:
    candidate = Path(path)
    if candidate.is_file():
        runpy.run_path(str(candidate), run_name="__main__")


def main() -> None:
    run_if_present("scripts/apply_issue22_docs.py")
    run_if_present("scripts/fix_issue22_static.py")
    quant_path = Path("backend/src/runners/quant_experiments.py")
    quant_text = quant_path.read_text(encoding="utf-8") if quant_path.is_file() else ""
    if "def _same_universe" not in quant_text:
        run_if_present("scripts/fix_issue22_sealed.py")
    run_if_present("scripts/harden_issue22_runtime.py")
    run_if_present("scripts/finalize_issue22_pipeline_tests.py")

    for path in Path("scripts").glob("*issue22*.py"):
        path.unlink(missing_ok=True)
    for path in Path(".github/workflows").glob("issue22-*.yml"):
        path.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
