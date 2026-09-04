from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select

from db.models import (
    CapitalContextVersion,
    DatasetRevision,
    DownstreamSystem,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    Event,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    PortfolioMandate,
    PortfolioMandateVersion,
    PreflightReceipt,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    PublicMutationReceipt,
)
from db.session import create_session_factory
from main import create_app
from settings import Settings


def _v1_mandate(universe_version_id: str) -> dict[str, object]:
    return {
        "policy_family": "LONG_ONLY_MEAN_VARIANCE_V1",
        "base_currency": "USD",
        "objective": "MAXIMIZE_NET_RETURN",
        "eligible_alpha_role": "PRIMARY_ALPHA",
        "universe_version_id": universe_version_id,
        "minimum_alpha_count": 2,
        "minimum_weight": "0.10",
        "maximum_weight": "0.75",
        "gross_exposure_limit": "1",
        "net_exposure_target": "1",
        "cash_reserve": "0",
        "turnover_limit": "1",
        "variance_limit": "0.25",
        "risk_aversion": "1",
        "cost_aversion": "0.50",
        "uncertainty_aversion": "0.25",
        "commission_rate": "0.0001",
        "half_spread_rate": "0.0002",
        "slippage_rate": "0.0001",
        "impact_rate": "0.001",
        "impact_breakpoint": "0.05",
        "state": "ACTIVE",
    }


def _evaluation_design(universe_version_id: str) -> dict[str, object]:
    return {
        "universe_version_id": universe_version_id,
        "contract_version": "evaluation-design-v1",
        "allowed_model_mode": "RELATIVE_SCORE",
        "qualification_role": "PRIMARY_ALPHA",
        "walk_forward_folds": 2,
        "annualization_factor": "252",
        "multiple_testing_method": "BONFERRONI",
        "multiple_testing_max_trials": 3,
        "qualification_metric_code": "SHARPE_RATIO",
        "qualification_comparator": "MINIMUM",
        "qualification_threshold": "0.01",
        "pass_disclosure_code": "QUALIFIED",
        "failure_disclosure_code": "INSUFFICIENT_NET_EDGE",
        "inconclusive_disclosure_code": "INCONCLUSIVE",
        "invalid_disclosure_code": "DATA_QUALITY_FAILURE",
        "state": "ACTIVE",
    }


def _promotion_policy() -> dict[str, object]:
    return {
        "purpose": "SEALED_TO_QUALIFIED",
        "mode": "MANUAL_APPROVAL",
        "gates": [
            {
                "metric_code": "SHARPE_RATIO",
                "comparator": "MINIMUM",
                "threshold": "1.0",
                "ordinal": 2,
            },
            {
                "metric_code": "NET_RETURN",
                "comparator": "MINIMUM",
                "threshold": "0.01",
                "ordinal": 1,
            },
        ],
        "state": "ACTIVE",
    }


def _seed_trusted_alpha_configuration(engine: Engine) -> dict[str, str]:
    now = datetime.now(UTC)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        universe = MarketUniverseVersion(
            universe_key="TRUSTED_ALPHA",
            version_no=1,
            name="Trusted Alpha Universe",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        other_universe = MarketUniverseVersion(
            universe_key="OTHER_ALPHA",
            version_no=1,
            name="Other Alpha Universe",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        source = GovernedDataSource(
            name="Trusted Alpha Source",
            connector_key="trusted-alpha-source",
            provider="Fixture",
            state="ACTIVE",
            universe_scope=[],
            fields=[],
            field_schema={},
            license_classification="FIXTURE",
            availability_semantics={},
            preflight_state="READY",
            public_config={},
        )
        session.add_all((universe, other_universe, source))
        session.flush()

        def dataset(
            partition: str, revision_no: int, *, universe_id: UUID = universe.id
        ) -> DatasetRevision:
            item = DatasetRevision(
                data_source_id=source.id,
                universe_version_id=universe_id,
                universe_name=universe.name,
                revision_no=revision_no,
                data_class="VENDOR",
                origin="fixture",
                ingested_at=now,
                promotability="PROMOTABLE",
                schema_version="v1",
                event_start=now,
                event_end=now + timedelta(days=1),
                available_start=now,
                available_end=now + timedelta(days=1),
                row_count=10,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition=partition,
                materialization_request={},
                created_at=now,
            )
            session.add(item)
            session.flush()
            session.add(
                NautilusCatalogBinding(
                    dataset_revision_id=item.id,
                    catalog_uri=f"catalog://trusted-{partition.lower()}-{revision_no}-{universe_id}",
                    provider="fixture",
                    source_license="fixture",
                    nautilus_data_type="QuoteTick",
                    instrument_scope=["TEST"],
                    event_time_range={},
                    available_time_range={},
                    schema_revision="v1",
                    quality_state="VALID",
                    quality_result={},
                    point_in_time_state="VALID",
                    point_in_time_result={},
                    sealed=partition == "SEALED",
                )
            )
            return item

        discovery = dataset("DISCOVERY", 1)
        replacement_discovery = dataset("DISCOVERY", 2)
        validation = dataset("VALIDATION", 1)
        sealed = dataset("SEALED", 1)
        other_discovery = dataset("DISCOVERY", 1, universe_id=other_universe.id)
        contract = {
            "feedback_contract": {
                "minimum_observation_duration_seconds": 60,
                "minimum_valid_sample_size": 10,
                "required_fields": ["return"],
                "accepted_package_contracts": ["1"],
                "accepted_arrow_contracts": ["arrow-ipc-file-v1"],
                "disclosure_policy": "FULL",
            }
        }
        paper = DownstreamSystem(
            name="Trusted paper downstream",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["TARGET_PORTFOLIO_V1"],
            preflight_state="READY",
            revision=1,
            public_config=contract,
        )
        live = DownstreamSystem(
            name="Trusted live downstream",
            environment_type="LIVE",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["TARGET_PORTFOLIO_V1"],
            preflight_state="READY",
            revision=1,
            public_config=contract,
        )
        session.add_all((paper, live))
        session.flush()
        session.add_all(
            (
                PreflightReceipt(
                    resource_type="DOWNSTREAM_SYSTEM",
                    resource_id=paper.id,
                    resource_revision=paper.revision,
                    revision=1,
                    status="READY",
                    reason_codes=[],
                    capabilities=list(paper.compatibility),
                    contract_version=paper.feedback_contract_version,
                    checked_at=now,
                    valid_until=now + timedelta(days=1),
                    checker_version="fixture",
                ),
                PreflightReceipt(
                    resource_type="DOWNSTREAM_SYSTEM",
                    resource_id=live.id,
                    resource_revision=live.revision,
                    revision=1,
                    status="READY",
                    reason_codes=[],
                    capabilities=list(live.compatibility),
                    contract_version=live.feedback_contract_version,
                    checked_at=now,
                    valid_until=now + timedelta(days=1),
                    checker_version="fixture",
                ),
            )
        )
        return {
            "universe_id": str(universe.id),
            "discovery_id": str(discovery.id),
            "replacement_discovery_id": str(replacement_discovery.id),
            "validation_id": str(validation.id),
            "sealed_id": str(sealed.id),
            "other_discovery_id": str(other_discovery.id),
            "paper_downstream_id": str(paper.id),
            "live_downstream_id": str(live.id),
        }


def test_empty_sqlite_can_create_immutable_configuration_facts(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    assert client.get("/api/v1/configuration/universes").status_code == 404
    assert client.get("/api/v1/universes").json() == {
        "items": [],
        "next_cursor": None,
    }

    universe = client.post(
        "/api/v1/universes",
        headers={"Idempotency-Key": "universe-1"},
        json={
            "universe_key": "US_EQUITIES",
            "name": "US Equities",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"listing": "NYSE|NASDAQ"},
            "calendar_semantics": {"timezone": "America/New_York"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    universe_id = universe.json()["id"]

    universe_v2 = client.post(
        f"/api/v1/universes/{universe_id}/versions",
        json={
            "name": "US Equities v2",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"listing": "NYSE|NASDAQ"},
            "calendar_semantics": {"timezone": "America/New_York"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe_v2.status_code == 201, universe_v2.text
    assert universe_v2.json()["universe_key"] == "US_EQUITIES"
    assert universe_v2.json()["version_no"] == 2

    source = client.post(
        "/api/v1/data-sources",
        json={
            "name": "Licensed bars",
            "connector_key": "licensed-bars",
            "provider": "Example Vendor",
            "universe_scope": [universe_id],
            "field_schema": {"timestamp": "timestamp", "close": "decimal"},
            "license_classification": "LICENSED",
            "availability_semantics": {"available_at_field": "received_at"},
            "public_config": {"dataset": "daily-bars"},
        },
    )
    assert source.status_code == 201, source.text
    source_id = source.json()["id"]
    assert source.json()["preflight_state"] == "PENDING"
    assert client.get("/api/v1/data-sources").json()["items"]

    materialization = client.post(
        "/api/v1/datasets/materializations",
        headers={"Idempotency-Key": "dataset-1"},
        json={
            "data_source_id": source_id,
            "universe_version_id": universe_id,
            "partition": "DISCOVERY",
            "data_class": "VENDOR",
            "origin": "vendor-materialization",
            "schema_version": "bars-v1",
            "data_type": "BAR",
            "instrument_scope": ["AAPL.XNAS"],
            "event_start": "2025-01-01T00:00:00Z",
            "event_end": "2025-01-02T00:00:00Z",
            "available_start": "2025-01-01T00:05:00Z",
            "available_end": "2025-01-02T00:05:00Z",
            "quality_requirements": {"minimum_coverage": 1.0},
            "point_in_time_requirements": {"available_at": "required"},
        },
    )
    assert materialization.status_code == 409, materialization.text
    assert materialization.json()["error"]["code"] == "DATA_SOURCE_PREFLIGHT_REQUIRED"

    datasets = client.get("/api/v1/datasets")
    assert datasets.status_code == 200
    assert datasets.json() == {"items": [], "next_cursor": None}

    mandate = client.post(
        "/api/v1/portfolio-mandates",
        json={
            "key": "core-growth",
            "name": "Core Growth",
            "enabled": True,
            **_v1_mandate(universe_id),
        },
    )
    assert mandate.status_code == 201, mandate.text
    mandate_id = mandate.json()["id"]
    assert mandate.json()["latest_version"]["version_no"] == 1
    assert mandate.json()["configuration_state"] == "V1_CONFIGURED"
    assert client.get("/api/v1/portfolio-mandates").json()["items"]

    mandate_v2 = client.post(
        f"/api/v1/portfolio-mandates/{mandate_id}/versions",
        json=_v1_mandate(universe_v2.json()["id"]),
    )
    assert mandate_v2.status_code == 201, mandate_v2.text
    assert mandate_v2.json()["latest_version"]["version_no"] == 2

    capital = client.post(
        "/api/v1/capital-contexts",
        headers={"Idempotency-Key": "capital-context-1"},
        json={
            "base_currency": "USD",
            "deployable_capital": "100000.0001",
            "observed_at": "2026-09-03T00:00:00Z",
            "valid_until": "2026-10-03T00:00:00Z",
            "notes": "Operator budget snapshot",
        },
    )
    assert capital.status_code == 201, capital.text
    assert capital.json()["configuration_contract_version"] == "CAPITAL_CONTEXT_V1"
    assert capital.json()["configuration_state"] == "V1_CONFIGURED"
    capital_replay = client.post(
        "/api/v1/capital-contexts",
        headers={"Idempotency-Key": "capital-context-1"},
        json={
            "base_currency": "USD",
            "deployable_capital": "100000.0001",
            "observed_at": "2026-09-03T00:00:00Z",
            "valid_until": "2026-10-03T00:00:00Z",
            "notes": "Operator budget snapshot",
        },
    )
    assert capital_replay.status_code == 201, capital_replay.text
    assert capital_replay.json()["id"] == capital.json()["id"]
    assert client.get("/api/v1/capital-contexts").json()["items"]

    downstream = client.post(
        "/api/v1/downstream-systems",
        headers={"Idempotency-Key": "paper-downstream-1"},
        json={
            "name": "Paper consumer",
            "environment_type": "PAPER",
            "public_config": {"endpoint": "https://paper.example.invalid"},
        },
    )
    assert downstream.status_code == 201, downstream.text
    assert downstream.json()["service_token"]
    assert downstream.json()["preflight_state"] == "PENDING"
    replay = client.post(
        "/api/v1/downstream-systems",
        headers={"Idempotency-Key": "paper-downstream-1"},
        json={
            "name": "Paper consumer",
            "environment_type": "PAPER",
            "public_config": {"endpoint": "https://paper.example.invalid"},
        },
    )
    assert replay.status_code == 201, replay.text
    assert replay.json()["service_token"] is None
    assert client.get("/api/v1/downstream-systems").json()["items"]
    rotated = client.post(
        f"/api/v1/downstream-systems/{downstream.json()['id']}/rotate-service-token",
        headers={"Idempotency-Key": "paper-downstream-rotate-1"},
        json={},
    )
    assert rotated.status_code == 200, rotated.text
    assert rotated.json()["service_token"]
    rotated_replay = client.post(
        f"/api/v1/downstream-systems/{downstream.json()['id']}/rotate-service-token",
        headers={"Idempotency-Key": "paper-downstream-rotate-1"},
        json={},
    )
    assert rotated_replay.status_code == 200, rotated_replay.text
    assert rotated_replay.json()["service_token"] is None

    factory = create_session_factory(engine)
    with factory() as session:
        receipt = session.get(PublicMutationReceipt, "paper-downstream-1")
        assert receipt is not None
        assert receipt.response_json["service_token"] is None
        rotated_receipt = session.get(PublicMutationReceipt, "paper-downstream-rotate-1")
        assert rotated_receipt is not None
        assert rotated_receipt.response_json["service_token"] is None


def test_typed_portfolio_configuration_rejects_legacy_inputs_and_marks_legacy_facts(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    rejected_mandate = client.post(
        "/api/v1/portfolio-mandates",
        json={
            "key": "legacy-shape",
            "name": "Legacy shape",
            "base_currency": "USD",
            "objective": "MAXIMIZE_NET_RETURN",
            "eligible_alpha_roles": ["PRIMARY_ALPHA"],
            "eligible_universe_version_ids": [str(uuid4())],
            "minimum_alpha_count": 2,
            "capital_config": {"deployable_capital": 1},
        },
    )
    assert rejected_mandate.status_code == 422

    zero_minimum_weight = client.post(
        "/api/v1/portfolio-mandates",
        json={
            "key": "zero-minimum-weight",
            "name": "Zero Minimum Weight",
            "enabled": True,
            **_v1_mandate(str(uuid4())),
            "minimum_weight": "0",
        },
    )
    assert zero_minimum_weight.status_code == 422

    rejected_capital = client.post(
        "/api/v1/capital-contexts",
        json={
            "base_currency": "USD",
            "deployable_capital": 1000,
            "observed_at": "2026-09-03T00:00:00Z",
            "valid_until": "2026-10-03T00:00:00Z",
        },
    )
    assert rejected_capital.status_code == 422

    mandate_id = uuid4()
    version_id = uuid4()
    factory = create_session_factory(engine)
    with factory.begin() as session:
        session.add(
            PortfolioMandate(
                id=mandate_id,
                key="legacy-mandate",
                name="Legacy Mandate",
                enabled=True,
                latest_version_id=version_id,
                spec_json={},
                state="ACTIVE",
            )
        )
        session.flush()
        session.add(
            PortfolioMandateVersion(
                id=version_id,
                portfolio_mandate_id=mandate_id,
                version_no=1,
                base_currency="USD",
                objective="MAXIMIZE_NET_RETURN",
                eligible_alpha_roles=["PRIMARY_ALPHA"],
                eligible_universe_version_ids=[str(uuid4())],
                minimum_alpha_count=2,
                capital_config={"deployable_capital": 1},
                risk_config={"model": "LEGACY"},
                cost_config={"model": "LEGACY"},
                capacity_config={"model": "LEGACY"},
                promotion_policy={"paper": "LEGACY"},
                constraint_config={"gross_exposure_limit": 1},
            )
        )
        session.add(
            CapitalContextVersion(
                source_type="ADMIN",
                base_currency="USD",
                deployable_capital=Decimal("1000"),
                observed_at=datetime(2026, 9, 3, tzinfo=UTC),
                valid_until=datetime(2026, 10, 3, tzinfo=UTC),
            )
        )

    mandates = client.get("/api/v1/portfolio-mandates")
    assert mandates.status_code == 200, mandates.text
    legacy_mandate = next(item for item in mandates.json()["items"] if item["id"] == str(mandate_id))
    assert legacy_mandate["configuration_state"] == "LEGACY_UNAVAILABLE"
    assert legacy_mandate["latest_version"] is None

    capitals = client.get("/api/v1/capital-contexts")
    assert capitals.status_code == 200, capitals.text
    assert capitals.json()["items"][0]["configuration_state"] == "LEGACY_UNAVAILABLE"


def test_downstream_service_preflight_is_authenticated_exact_and_rotation_bound(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    registered = client.post(
        "/api/v1/downstream-systems",
        json={
            "name": "Preflight consumer",
            "environment_type": "PAPER",
            "compatibility": ["TARGET_PORTFOLIO_V1"],
            "public_config": {
                "feedback_contract": {
                    "minimum_observation_duration_seconds": 60,
                    "minimum_valid_sample_size": 10,
                    "required_fields": ["return"],
                    "accepted_package_contracts": ["1"],
                    "accepted_arrow_contracts": ["arrow-ipc-file-v1"],
                    "disclosure_policy": "FULL",
                }
            },
        },
    )
    assert registered.status_code == 201, registered.text
    downstream = registered.json()
    downstream_id = downstream["id"]
    token = downstream["service_token"]
    assert token
    path = f"/api/v1/downstream-systems/{downstream_id}/preflight"
    payload = {
        "package_contract_version": "1",
        "feedback_contract_version": "1",
        "compatibility": ["TARGET_PORTFOLIO_V1"],
        "valid_until": (datetime.now(UTC) + timedelta(days=1)).isoformat(),
    }

    missing_auth = client.post(path, json=payload)
    assert missing_auth.status_code == 401
    assert missing_auth.json()["error"]["code"] == "DOWNSTREAM_AUTH_REQUIRED"

    mismatch = client.post(
        path,
        headers={"Authorization": f"Bearer {token}"},
        json={**payload, "compatibility": ["WRONG"]},
    )
    assert mismatch.status_code == 409
    assert mismatch.json()["error"]["code"] == "DOWNSTREAM_PREFLIGHT_CONTRACT_MISMATCH"

    non_utc = client.post(
        path,
        headers={"Authorization": f"Bearer {token}"},
        json={**payload, "valid_until": "2030-01-01T00:00:00+01:00"},
    )
    assert non_utc.status_code == 422
    unknown_input = client.post(
        path,
        headers={"Authorization": f"Bearer {token}"},
        json={**payload, "endpoint": "https://consumer.example.invalid"},
    )
    assert unknown_input.status_code == 422

    headers = {
        "Authorization": f"Bearer {token}",
        "Idempotency-Key": "downstream-preflight-1",
    }
    ready = client.post(path, headers=headers, json=payload)
    assert ready.status_code == 200, ready.text
    assert ready.json()["preflight_state"] == "READY"
    replay = client.post(path, headers=headers, json=payload)
    assert replay.status_code == 200, replay.text
    assert replay.json()["preflight_state"] == "READY"

    factory = create_session_factory(engine)
    with factory() as session:
        receipts = list(
            session.scalars(
                select(PreflightReceipt)
                .where(
                    PreflightReceipt.resource_type == "DOWNSTREAM_SYSTEM",
                    PreflightReceipt.resource_id == UUID(downstream_id),
                )
                .order_by(PreflightReceipt.revision)
            )
        )
        assert [(receipt.resource_revision, receipt.revision) for receipt in receipts] == [(1, 1)]
        assert receipts[0].contract_version == "1"
        assert receipts[0].capabilities == ["TARGET_PORTFOLIO_V1"]

    rotated = client.post(
        f"/api/v1/downstream-systems/{downstream_id}/rotate-service-token",
        json={},
    )
    assert rotated.status_code == 200, rotated.text
    replacement_token = rotated.json()["service_token"]
    assert replacement_token
    assert client.get("/api/v1/downstream-systems").json()["items"][0]["preflight_state"] == "PENDING"

    old_token = client.post(
        path,
        headers={"Authorization": f"Bearer {token}"},
        json=payload,
    )
    assert old_token.status_code == 403
    assert old_token.json()["error"]["code"] == "DOWNSTREAM_UNAUTHORIZED"
    renewed = client.post(
        path,
        headers={
            "Authorization": f"Bearer {replacement_token}",
            "Idempotency-Key": "downstream-preflight-2",
        },
        json=payload,
    )
    assert renewed.status_code == 200, renewed.text

    with factory() as session:
        system = session.get(DownstreamSystem, UUID(downstream_id))
        assert system is not None
        assert (system.revision, system.preflight_state) == (2, "READY")
        receipts = list(
            session.scalars(
                select(PreflightReceipt)
                .where(
                    PreflightReceipt.resource_type == "DOWNSTREAM_SYSTEM",
                    PreflightReceipt.resource_id == system.id,
                )
                .order_by(PreflightReceipt.revision)
            )
        )
        assert [(receipt.resource_revision, receipt.revision) for receipt in receipts] == [
            (1, 1),
            (2, 2),
        ]


def test_downstream_preflight_rejects_implicit_feedback_contract_defaults(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    registered = client.post(
        "/api/v1/downstream-systems",
        json={"name": "Incomplete consumer", "environment_type": "PAPER"},
    )
    assert registered.status_code == 201, registered.text
    downstream = registered.json()
    response = client.post(
        f"/api/v1/downstream-systems/{downstream['id']}/preflight",
        headers={"Authorization": f"Bearer {downstream['service_token']}"},
        json={
            "package_contract_version": "1",
            "feedback_contract_version": "1",
            "compatibility": [],
            "valid_until": (datetime.now(UTC) + timedelta(days=1)).isoformat(),
        },
    )

    assert response.status_code == 422
    assert response.json()["error"]["code"] == "FEEDBACK_CONTRACT_INVALID"
    factory = create_session_factory(engine)
    with factory() as session:
        system = session.get(DownstreamSystem, UUID(downstream["id"]))
        assert system is not None
        assert system.preflight_state == "PENDING"
        assert (
            session.scalar(
                select(PreflightReceipt.id).where(PreflightReceipt.resource_id == system.id)
            )
            is None
        )


def test_data_source_preflight_is_an_empty_async_canonical_operation(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": "PMXT",
            "name": "PMXT Universe",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"market": "all"},
            "calendar_semantics": {"timezone": "UTC"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    source = client.post(
        "/api/v1/data-sources",
        json={
            "name": "PMXT archive",
            "connector_key": "pmxt_archive",
            "provider": "PMXT",
            "universe_scope": [universe.json()["id"]],
            "field_schema": {"timestamp_received": "timestamp"},
            "license_classification": "PMXT_PUBLIC",
            "availability_semantics": {"available_at_field": "timestamp_received"},
            "public_config": {
                "source_spec": {
                    "kind": "plugin",
                    "config": {
                        "venue": "polymarket_v2",
                        "selection": "all_markets",
                        "archive_start": "2026-01-01T00:00:00Z",
                        "archive_end": "2026-01-02T00:00:00Z",
                    },
                },
                "plugin_binding": {
                    "plugin_release_id": "00000000-0000-0000-0000-000000000001",
                    "plugin_runtime_bundle_id": "00000000-0000-0000-0000-000000000002",
                },
            },
        },
    )
    assert source.status_code == 201, source.text
    path = f"/api/v1/data-sources/{source.json()['id']}/preflight"
    queued = client.post(path, headers={"Idempotency-Key": "pmxt-preflight-1"}, json={})
    assert queued.status_code == 202, queued.text
    operation = queued.json()
    assert operation["kind"] == "DATA_SOURCE_PREFLIGHT"
    assert operation["resource_type"] == "governed_data_source"
    assert operation["resource_id"] == source.json()["id"]
    assert operation["state"] == "READY"

    fetched = client.get(f"/api/v1/operations/{operation['id']}")
    assert fetched.status_code == 200, fetched.text
    assert fetched.json() == operation
    replay = client.post(path, headers={"Idempotency-Key": "pmxt-preflight-1"}, json={})
    assert replay.status_code == 202, replay.text
    assert replay.json() == operation
    in_progress = client.post(path, json={})
    assert in_progress.status_code == 409, in_progress.text
    assert in_progress.json()["error"]["code"] == "DATA_SOURCE_PREFLIGHT_IN_PROGRESS"

    factory = create_session_factory(engine)
    with factory() as session:
        job = session.get(Job, UUID(operation["id"]))
        assert job is not None
        assert job.payload == {}
        event = session.scalar(select(Event).where(Event.kind == "DATA_SOURCE_PREFLIGHT_REQUESTED"))
        assert event is not None
        assert event.payload == {"job_id": operation["id"]}
    with factory.begin() as session:
        job = session.get(Job, UUID(operation["id"]))
        assert job is not None
        job.state = "FAILED"
    failed = client.get(f"/api/v1/operations/{operation['id']}")
    assert failed.status_code == 200, failed.text
    assert failed.json()["state"] == "FAILED"
    retry = client.post(path, headers={"Idempotency-Key": "pmxt-preflight-2"}, json={})
    assert retry.status_code == 202, retry.text
    assert retry.json()["id"] != operation["id"]
    assert retry.json()["state"] == "READY"

    raw_url = client.post(path, json={"archive_url": "https://example.invalid/archive.parquet"})
    assert raw_url.status_code == 422
    credential = client.post(path, json={"api_key": "not-accepted"})
    assert credential.status_code == 422


def test_trusted_alpha_configuration_writers_are_immutable_and_readable(
    engine: Engine, settings: Settings
) -> None:
    facts = _seed_trusted_alpha_configuration(engine)
    client = TestClient(create_app(settings=settings, engine=engine))
    selection_payload = {
        "universe_version_id": facts["universe_id"],
        "discovery_dataset_revision_id": facts["discovery_id"],
        "validation_dataset_revision_id": facts["validation_id"],
        "sealed_dataset_revision_id": facts["sealed_id"],
        "state": "ENABLED",
    }
    selection = client.post(
        "/api/v1/evaluation-dataset-selections",
        headers={"Idempotency-Key": "selection-v1"},
        json=selection_payload,
    )
    assert selection.status_code == 201, selection.text
    selection_replay = client.post(
        "/api/v1/evaluation-dataset-selections",
        headers={"Idempotency-Key": "selection-v1"},
        json=selection_payload,
    )
    assert selection_replay.status_code == 201, selection_replay.text
    assert selection_replay.json()["id"] == selection.json()["id"]
    replacement_selection = client.post(
        "/api/v1/evaluation-dataset-selections",
        json={
            **selection_payload,
            "discovery_dataset_revision_id": facts["replacement_discovery_id"],
        },
    )
    assert replacement_selection.status_code == 201, replacement_selection.text
    assert replacement_selection.json()["version_no"] == 2
    selections = {
        item["id"]: item
        for item in client.get("/api/v1/evaluation-dataset-selections").json()["items"]
    }
    assert selections[selection.json()["id"]]["state"] == "RETIRED"
    assert selections[replacement_selection.json()["id"]]["state"] == "ENABLED"

    design_payload = _evaluation_design(facts["universe_id"])
    design = client.post(
        "/api/v1/evaluation-design-versions",
        headers={"Idempotency-Key": "design-v1"},
        json=design_payload,
    )
    assert design.status_code == 201, design.text
    replacement_design = client.post(
        "/api/v1/evaluation-design-versions",
        json={**design_payload, "qualification_threshold": "0.02"},
    )
    assert replacement_design.status_code == 201, replacement_design.text
    designs = {
        item["id"]: item
        for item in client.get("/api/v1/evaluation-design-versions").json()["items"]
    }
    assert designs[design.json()["id"]]["state"] == "RETIRED"
    assert designs[replacement_design.json()["id"]]["state"] == "ACTIVE"

    policy_payload = _promotion_policy()
    policy = client.post(
        "/api/v1/promotion-policy-versions",
        headers={"Idempotency-Key": "policy-v1"},
        json=policy_payload,
    )
    assert policy.status_code == 201, policy.text
    assert [gate["ordinal"] for gate in policy.json()["gates"]] == [1, 2]
    assert policy.json()["policy_contract_version"] == "PROMOTION_POLICY_V1"
    assert policy.json()["paper_downstream_system_id"] is None
    assert policy.json()["live_downstream_system_id"] is None
    replacement_policy = client.post(
        "/api/v1/promotion-policy-versions",
        json={
            **policy_payload,
            "gates": [
                {**policy_payload["gates"][0], "threshold": "21"},
                policy_payload["gates"][1],
            ],
        },
    )
    assert replacement_policy.status_code == 201, replacement_policy.text
    policies = {
        item["id"]: item
        for item in client.get("/api/v1/promotion-policy-versions").json()["items"]
    }
    assert policies[policy.json()["id"]]["state"] == "RETIRED"
    assert policies[replacement_policy.json()["id"]]["state"] == "ACTIVE"

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.get(EvaluationDatasetSelection, UUID(selection.json()["id"])) is not None
        assert session.get(EvaluationDesignVersion, UUID(design.json()["id"])) is not None
        stored_policy = session.get(PromotionPolicyVersion, UUID(policy.json()["id"]))
        assert stored_policy is not None
        gates = list(
            session.scalars(
                select(PromotionPolicyGate)
                .where(PromotionPolicyGate.policy_version_id == stored_policy.id)
                .order_by(PromotionPolicyGate.ordinal)
            )
        )
        assert [(gate.metric_code, gate.ordinal) for gate in gates] == [
            ("NET_RETURN", 1),
            ("SHARPE_RATIO", 2),
        ]


def test_trusted_alpha_configuration_rejects_implicit_or_untrusted_inputs(
    engine: Engine, settings: Settings
) -> None:
    facts = _seed_trusted_alpha_configuration(engine)
    client = TestClient(create_app(settings=settings, engine=engine))
    selection_payload = {
        "universe_version_id": facts["universe_id"],
        "discovery_dataset_revision_id": facts["discovery_id"],
        "validation_dataset_revision_id": facts["validation_id"],
        "sealed_dataset_revision_id": facts["sealed_id"],
        "state": "ENABLED",
    }
    assert (
        client.post(
            "/api/v1/evaluation-dataset-selections",
            json={key: value for key, value in selection_payload.items() if key != "state"},
        ).status_code
        == 422
    )
    wrong_universe = client.post(
        "/api/v1/evaluation-dataset-selections",
        json={**selection_payload, "discovery_dataset_revision_id": facts["other_discovery_id"]},
    )
    assert wrong_universe.status_code == 409
    assert wrong_universe.json()["error"]["code"] == "EVALUATION_DATASET_INVALID"

    factory = create_session_factory(engine)
    with factory.begin() as session:
        catalog = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == UUID(facts["sealed_id"])
            )
        )
        assert catalog is not None
        catalog.sealed = False
    invalid_sealed = client.post("/api/v1/evaluation-dataset-selections", json=selection_payload)
    assert invalid_sealed.status_code == 409
    assert invalid_sealed.json()["error"]["code"] == "EVALUATION_DATASET_INVALID"

    design_payload = _evaluation_design(facts["universe_id"])
    assert (
        client.post(
            "/api/v1/evaluation-design-versions",
            json={**design_payload, "annualization_factor": 252},
        ).status_code
        == 422
    )
    assert (
        client.post(
            "/api/v1/evaluation-design-versions",
            json={key: value for key, value in design_payload.items() if key != "state"},
        ).status_code
        == 422
    )
    assert (
        client.post(
            "/api/v1/evaluation-design-versions",
            json={**design_payload, "allowed_model_mode": "CALIBRATED_RETURN"},
        ).status_code
        == 422
    )
    assert (
        client.post(
            "/api/v1/evaluation-design-versions",
            json={**design_payload, "qualification_metric_code": "TURNOVER"},
        ).status_code
        == 422
    )

    policy_payload = _promotion_policy()
    assert (
        client.post(
            "/api/v1/promotion-policy-versions",
            json={**policy_payload, "gates": [{**policy_payload["gates"][0], "threshold": 20}]},
        ).status_code
        == 422
    )
    assert (
        client.post(
            "/api/v1/promotion-policy-versions",
            json={
                **policy_payload,
                "gates": [{**policy_payload["gates"][0], "metric_code": "TURNOVER"}],
            },
        ).status_code
        == 422
    )
    policies_before = client.get("/api/v1/promotion-policy-versions").json()["items"]
    unsupported_paper_policy = client.post(
        "/api/v1/promotion-policy-versions",
        json={**policy_payload, "purpose": "PORTFOLIO_TO_PAPER"},
    )
    assert unsupported_paper_policy.status_code == 409
    assert unsupported_paper_policy.json()["error"]["code"] == "PROMOTION_POLICY_TYPED_BINDING_UNAVAILABLE"
    unsupported_live_policy = client.post(
        "/api/v1/promotion-policy-versions",
        json={**policy_payload, "purpose": "PAPER_TO_LIVE"},
    )
    assert unsupported_live_policy.status_code == 409
    assert unsupported_live_policy.json()["error"]["code"] == "PROMOTION_POLICY_TYPED_BINDING_UNAVAILABLE"
    assert client.get("/api/v1/promotion-policy-versions").json()["items"] == policies_before
    assert (
        client.post(
            "/api/v1/promotion-policy-versions",
            json={**policy_payload, "paper_downstream_system_id": facts["paper_downstream_id"]},
        ).status_code
        == 422
    )
