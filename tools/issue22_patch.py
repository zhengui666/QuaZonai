from pathlib import Path

def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")

def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")

def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise SystemExit(f"pattern missing in {path}: {old[:140]!r}")
    write(path, text.replace(old, new, 1))

bundle = "backend/src/candidate_bundles.py"
replace_once(bundle, "def _strategy_wheel(artifact: StrategyArtifact, *, candidate_id: UUID) -> bytes:\n", '''def _strategy_wheel_filename(candidate_id: UUID) -> str:
    version = f"0.0.{candidate_id.int % 1_000_000}"
    return f"quazonai_candidate_strategy-{version}-py3-none-any.whl"


def _strategy_wheel(artifact: StrategyArtifact, *, candidate_id: UUID) -> bytes:
''')
replace_once(bundle, '''    bundle_id = bundle_id or uuid4()
    target_weights = _member_payload(candidate)
''', '''    bundle_id = bundle_id or uuid4()
    strategy_wheel_path = f"strategy/{_strategy_wheel_filename(candidate_id)}"
    target_weights = _member_payload(candidate)
''')
replace_once(bundle, '            "strategy_wheel": "strategy/strategy.whl",\n', '            "strategy_wheel": strategy_wheel_path,\n')
replace_once(bundle, '                "wheel": "strategy/strategy.whl",\n', '                "wheel": strategy_wheel_path,\n')
replace_once(bundle, '        "strategy/strategy.whl": _strategy_wheel(artifact, candidate_id=candidate_id),\n', '        strategy_wheel_path: _strategy_wheel(artifact, candidate_id=candidate_id),\n')
replace_once(bundle, '        wheel = archive.read("strategy/strategy.whl")\n', '''        wheel_path = str(built.manifest.get("strategy", {}).get("wheel", ""))
        if not wheel_path:
            raise QfError(
                "CANDIDATE_CONFORMANCE_WHEEL_MISSING",
                "Candidate manifest does not identify its installable strategy wheel.",
                500,
            )
        wheel = archive.read(wheel_path)
''')
replace_once(bundle, '        "strategy/strategy.whl",\n', "")
replace_once(bundle, '''                if manifest.get("strategy", {}).get("wheel") != "strategy/strategy.whl":
                    findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
''', '''                wheel_path = str(manifest.get("strategy", {}).get("wheel", ""))
                try:
                    candidate_id = UUID(str(manifest.get("candidate_id")))
                    expected_wheel = f"strategy/{_strategy_wheel_filename(candidate_id)}"
                except ValueError:
                    expected_wheel = ""
                if wheel_path != expected_wheel or wheel_path not in names:
                    findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
''')
engine = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
replace_once(engine, '''        strategy = request.manifest.get("strategy", {})
        if strategy.get("wheel") != "strategy/strategy.whl":
            findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
''', '''        strategy = request.manifest.get("strategy", {})
        expected_wheel = (
            "strategy/quazonai_candidate_strategy-"
            f"0.0.{request.candidate_id.int % 1_000_000}-py3-none-any.whl"
        )
        if strategy.get("wheel") != expected_wheel:
            findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
''')
real_test = "nautilus_runtime/tests/test_real_backtest.py"
replace_once(real_test, '            "wheel": "strategy/strategy.whl",\n', '''            "wheel": (
                "strategy/quazonai_candidate_strategy-"
                f"0.0.{candidate_id.int % 1_000_000}-py3-none-any.whl"
            ),
''')
replace_once(bundle, "def _persist_bundle(settings: Any, bundle_id: UUID, archive_bytes: bytes) -> str:\n", "def persist_candidate_bundle(settings: Any, bundle_id: UUID, archive_bytes: bytes) -> str:\n")
replace_once(bundle, '''    return relative_path


@dataclass(frozen=True, slots=True)
class BuiltCandidateBundle:
''', '''    return relative_path


def discard_candidate_bundle(settings: Any, relative_path: str) -> None:
    package_root = getattr(settings, "package_root", None)
    if package_root is None:
        return
    root = Path(package_root).resolve()
    candidate = (root / relative_path).resolve()
    if not candidate.is_relative_to(root):
        return
    bundle_dir = candidate.parent
    if bundle_dir.parent != root:
        return
    if candidate.exists():
        candidate.unlink()
    if bundle_dir.exists() and not any(bundle_dir.iterdir()):
        bundle_dir.rmdir()


@dataclass(frozen=True, slots=True)
class BuiltCandidateBundle:
''')
replace_once(bundle, '''    bundle_id: UUID | None = None,
) -> BuiltCandidateBundle:
''', '''    bundle_id: UUID | None = None,
    persist: bool = True,
) -> BuiltCandidateBundle:
''')
replace_once(bundle, "    relative_path = _persist_bundle(settings, bundle_id, archive_bytes)\n", '''    relative_path = (Path(str(bundle_id)) / BUNDLE_FILENAME).as_posix()
    if persist:
        relative_path = persist_candidate_bundle(settings, bundle_id, archive_bytes)
''')
domain = "backend/src/api/domain.py"
replace_once(domain, '''    build_candidate_verification_request,
    resolve_bundle_archive,
''', '''    build_candidate_verification_request,
    discard_candidate_bundle,
    persist_candidate_bundle,
    resolve_bundle_archive,
''')
replace_once(domain, '''    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
''', '''    factory = request.app.state.session_factory
    persisted_bundle_path: str | None = None
    try:
        with factory() as session, session.begin():

            def action() -> dict[str, Any]:
''')
text = read(domain)
func_start = text.index("def approve_candidate(")
action_start = text.index("            def action() -> dict[str, Any]:\n", func_start)
route_marker = '\n\n@router.post("/approvals/{approval_id}/reject", response_model=ApprovalView)'
route_start = text.index(route_marker, action_start)
block = text[action_start:route_start]
lines = block.splitlines()
fixed = [lines[0]] + ["    " + line for line in lines[1:]]
text = text[:action_start] + "\n".join(fixed) + text[route_start:]
write(domain, text)
replace_once(domain, '''                    bundle_id=bundle_id,
                )
                _verify_candidate_bundle_remotely(built, candidate_id=candidate.id)
                package = CandidateBundle(
''', '''                    bundle_id=bundle_id,
                    persist=False,
                )
                _verify_candidate_bundle_remotely(built, candidate_id=candidate.id)
                persisted_bundle_path = persist_candidate_bundle(
                    request.app.state.settings,
                    bundle_id,
                    built.archive_bytes,
                )
                package = CandidateBundle(
''')
replace_once(domain, "                    relative_path=built.relative_path,\n", "                    relative_path=persisted_bundle_path,\n")
replace_once(domain, '''            def action() -> dict[str, Any]:
                approval = session.execute(
''', '''            def action() -> dict[str, Any]:
                nonlocal persisted_bundle_path
                approval = session.execute(
''')
text = read(domain)
route_start = text.index(route_marker, text.index("def approve_candidate("))
cleanup = '''\n    except Exception:
        if persisted_bundle_path is not None:
            discard_candidate_bundle(request.app.state.settings, persisted_bundle_path)
        raise
'''
text = text[:route_start] + cleanup + text[route_start:]
write(domain, text)
for test_path in ("backend/tests/unit/test_candidate_bundles.py", "backend/tests/integration/test_domain_api.py"):
    text = read(test_path)
    text = text.replace('            "strategy/strategy.whl",\n', "")
    text = text.replace('        "strategy/strategy.whl",\n', "")
    text = text.replace('        assert manifest["strategy"]["wheel"] == "strategy/strategy.whl"\n', '        assert manifest["strategy"]["wheel"] in names\n        assert manifest["strategy"]["wheel"].startswith("strategy/quazonai_candidate_strategy-")\n        assert manifest["strategy"]["wheel"].endswith("-py3-none-any.whl")\n')
    write(test_path, text)
print("stage2a issue22 bundle closure applied")
