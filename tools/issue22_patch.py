from pathlib import Path

def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")

def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")

def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise SystemExit(f"pattern missing in {path}: {old[:160]!r}")
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

domain = "backend/src/api/domain.py"
replace_once(domain, '''    build_candidate_verification_request,
    resolve_bundle_archive,
''', '''    build_candidate_verification_request,
    discard_candidate_bundle,
    persist_candidate_bundle,
    resolve_bundle_archive,
''')
text = read(domain)
start = text.index('@router.post("/approvals/{approval_id}/approve", response_model=ApprovalView)')
end = text.index('\n\n@router.post("/approvals/{approval_id}/reject", response_model=ApprovalView)', start)
new_function = '''@router.post("/approvals/{approval_id}/approve", response_model=ApprovalView)
def approve_candidate(
    approval_id: UUID,
    payload: ApprovalApproveInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    if _expire_approval_if_needed(request, approval_id):
        raise QfError("APPROVAL_EXPIRED", "Approval validity window has expired.", 409)
    factory = request.app.state.session_factory
    persisted_bundle_path: str | None = None
    try:
        with factory() as session, session.begin():

            def action() -> dict[str, Any]:
                nonlocal persisted_bundle_path
                approval = session.execute(
                    select(ApprovalSnapshot)
                    .where(ApprovalSnapshot.id == approval_id)
                    .with_for_update()
                ).scalar_one_or_none()
                if approval is None:
                    raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
                if approval.state != payload.expected_state or approval.state != "PENDING":
                    raise QfError(
                        "APPROVAL_STATE_CONFLICT",
                        "Approval state changed before the decision.",
                        409,
                        {"expected": payload.expected_state, "actual": approval.state},
                    )
                if (
                    approval.downstream_system_id is None
                    or approval.downstream_system_id != payload.downstream_system_id
                ):
                    raise QfError(
                        "APPROVAL_DOWNSTREAM_MISMATCH",
                        "Approval is frozen to a different Paper/Live downstream dependency.",
                        409,
                    )
                downstream = session.get(DownstreamSystem, payload.downstream_system_id)
                if (
                    downstream is None
                    or not downstream.enabled
                    or downstream.preflight_state != "READY"
                ):
                    raise QfError("DOWNSTREAM_NOT_READY", "Selected downstream is not ready.", 409)
                if downstream.environment_type != approval.purpose:
                    raise QfError(
                        "DOWNSTREAM_INCOMPATIBLE",
                        "Downstream environment does not match Approval purpose.",
                        409,
                    )
                if downstream.service_token_ciphertext is None:
                    raise QfError(
                        "DOWNSTREAM_CREDENTIAL_NOT_CONFIGURED",
                        "Selected downstream has no service credential.",
                        409,
                    )
                candidate = session.get(PortfolioCandidate, approval.candidate_id)
                if candidate is None:
                    raise QfError("CANDIDATE_NOT_FOUND", "Approval candidate was not found.", 500)
                approval.state = "APPROVED"
                approval.revision += 1
                bundle_id = uuid4()
                built = build_candidate_bundle(
                    request.app.state.settings,
                    approval=approval,
                    candidate=candidate,
                    downstream=downstream,
                    bundle_id=bundle_id,
                    persist=False,
                )
                _verify_candidate_bundle_remotely(built, candidate_id=candidate.id)
                persisted_bundle_path = persist_candidate_bundle(
                    request.app.state.settings,
                    bundle_id,
                    built.archive_bytes,
                )
                package = CandidateBundle(
                    id=bundle_id,
                    approval_id=approval.id,
                    candidate_id=candidate.id,
                    contract_version=BUNDLE_CONTRACT_VERSION,
                    state="AVAILABLE",
                    manifest_json=built.manifest,
                    relative_path=persisted_bundle_path,
                    payload=built.operator_summary,
                    created_at=_now(),
                )
                session.add(package)
                session.flush()
                contract = _feedback_contract_snapshot(downstream, approval.purpose)
                handoff = HandoffOffer(
                    approval_id=approval.id,
                    candidate_bundle_id=package.id,
                    candidate_id=candidate.id,
                    purpose=approval.purpose,
                    downstream_system_id=downstream.id,
                    state="AVAILABLE",
                    claim_deadline=_now() + timedelta(days=7),
                    feedback_state="PENDING",
                    feedback_contract_snapshot=contract,
                )
                session.add(handoff)
                session.flush()
                _event(
                    session,
                    "APPROVAL_APPROVED",
                    "APPROVAL",
                    approval.id,
                    {"candidate_id": str(candidate.id), "handoff_id": str(handoff.id)},
                    actor_kind="HUMAN",
                )
                _event(
                    session,
                    "HANDOFF_AVAILABLE",
                    "HANDOFF",
                    handoff.id,
                    {"candidate_id": str(candidate.id), "approval_id": str(approval.id)},
                )
                session.flush()
                return _approval_view(session, approval).model_dump(mode="json")

            return _idempotent(
                session,
                idempotency_key,
                f"approval.approve:{approval_id}",
                payload,
                action,
            )
    except Exception:
        if persisted_bundle_path is not None:
            discard_candidate_bundle(request.app.state.settings, persisted_bundle_path)
        raise
'''
write(domain, text[:start] + new_function + text[end:])

for test_path in ("backend/tests/unit/test_candidate_bundles.py", "backend/tests/integration/test_domain_api.py"):
    text = read(test_path)
    text = text.replace('            "strategy/strategy.whl",\n', "")
    text = text.replace('        "strategy/strategy.whl",\n', "")
    text = text.replace(
        '        assert manifest["strategy"]["wheel"] == "strategy/strategy.whl"\n',
        '        assert manifest["strategy"]["wheel"] in names\n'
        '        assert manifest["strategy"]["wheel"].startswith("strategy/quazonai_candidate_strategy-")\n'
        '        assert manifest["strategy"]["wheel"].endswith("-py3-none-any.whl")\n',
    )
    write(test_path, text)

print("stage2a issue22 bundle closure applied")
