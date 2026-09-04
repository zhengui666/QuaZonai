"""Persist real archive-manifest evidence before a Data Source becomes READY."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from typing import Any
from uuid import UUID

from sqlalchemy import select

from db.models import (
    ArchiveManifest,
    ArchiveManifestShard,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.session import SessionFactory, create_database_engine
from errors import QfError
from events import append_event
from jobs import JobLease, create_lease_fenced_session_factory
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import ArchiveManifestDescriptor, ArchiveManifestSpec
from quant_runtime.remote import NautilusQuantRuntime
from runtime_config import load_effective_settings
from settings import Settings

_SECRET_KEY_PARTS = frozenset(
    {
        "access_token",
        "api_key",
        "apikey",
        "password",
        "private_key",
        "refresh_token",
        "secret",
        "secret_key",
        "service_token",
        "token",
    }
)
_LOCATOR_KEY_PARTS = frozenset({"endpoint", "host", "hostname", "uri", "url"})


@dataclass(frozen=True)
class _PreflightContext:
    source_id: UUID
    universe_id: UUID
    manifest_name: str
    provider: str
    source_license: str
    source_spec: dict[str, Any]
    plugin_binding: dict[str, str]


def _runtime() -> NautilusQuantRuntime:
    config = RemoteNautilusConfig.from_env(required=True, profile="research")
    assert config is not None
    return NautilusQuantRuntime(config)


def _require_uuid(value: object, *, code: str, message: str) -> UUID:
    try:
        return UUID(str(value))
    except (TypeError, ValueError) as exc:
        raise QfError(code, message, 409) from exc


def _reject_untrusted_locator_or_secret(value: object, path: str = "$") -> None:
    """Archive scanners receive only plugin-owned public configuration."""
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if (
                normalized in _SECRET_KEY_PARTS
                or normalized.endswith("_secret")
                or normalized.endswith(("_token", "_key", "_password"))
                or "credential" in normalized
            ):
                raise QfError(
                    "DATA_SOURCE_PREFLIGHT_SECRET_FORBIDDEN",
                    "Data Source manifest configuration must not contain credentials.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            if normalized in _LOCATOR_KEY_PARTS or normalized.endswith(
                ("_url", "_uri", "_endpoint", "_host")
            ):
                raise QfError(
                    "DATA_SOURCE_PREFLIGHT_LOCATOR_FORBIDDEN",
                    "Data Source manifest configuration must not contain a remote locator.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            _reject_untrusted_locator_or_secret(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_untrusted_locator_or_secret(item, f"{path}[{index}]")
    elif isinstance(value, str) and "://" in value:
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_LOCATOR_FORBIDDEN",
            "Data Source manifest configuration must not contain a remote locator.",
            422,
            {"path": path},
        )


def _resolve_active_importer_binding(session: Any, binding: dict[str, Any]) -> dict[str, str]:
    release_id = _require_uuid(
        binding.get("plugin_release_id"),
        code="DATA_SOURCE_PREFLIGHT_BINDING_INVALID",
        message="Data Source plugin release binding is invalid.",
    )
    bundle_id = _require_uuid(
        binding.get("plugin_runtime_bundle_id"),
        code="DATA_SOURCE_PREFLIGHT_BINDING_INVALID",
        message="Data Source plugin runtime bundle binding is invalid.",
    )
    release = session.get(PluginRelease, release_id)
    if release is None or release.state != "ACTIVE":
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_PLUGIN_RELEASE_NOT_ACTIVE",
            "Data Source preflight requires an ACTIVE plugin release.",
            409,
        )
    capabilities = release.descriptor_snapshot.get("capabilities", [])
    if not isinstance(capabilities, list) or "HISTORICAL_IMPORT" not in capabilities:
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_PLUGIN_CAPABILITY_FORBIDDEN",
            "Data Source preflight requires historical-import capability.",
            422,
        )
    bundle = session.get(PluginRuntimeBundle, bundle_id)
    if bundle is None or bundle.state != "READY":
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_PLUGIN_BUNDLE_NOT_READY",
            "Data Source preflight requires a READY plugin runtime bundle.",
            409,
        )
    member = session.scalar(
        select(PluginRuntimeBundleMember).where(
            PluginRuntimeBundleMember.runtime_bundle_id == bundle.id,
            PluginRuntimeBundleMember.plugin_release_id == release.id,
            PluginRuntimeBundleMember.member_role == "IMPORTER",
        )
    )
    if member is None:
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_PLUGIN_BUNDLE_MEMBER_REQUIRED",
            "The plugin runtime bundle must include the release as an IMPORTER.",
            422,
        )
    return {
        "plugin_release_id": str(release.id),
        "plugin_runtime_bundle_id": str(bundle.id),
        "plugin_id": release.plugin_id,
        "plugin_version": release.version,
        "plugin_bundle_path": bundle.environment_path,
    }


def _prepare_preflight(factory: SessionFactory, job_id: UUID) -> _PreflightContext:
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None or job.kind != "DATA_SOURCE_PREFLIGHT":
            raise QfError("JOB_NOT_FOUND", "Data Source preflight job does not exist.", 404)
        if job.state != "LEASED":
            raise QfError("JOB_STATE_CONFLICT", "Data Source preflight job is not leased.", 409)
        if job.resource_type != "governed_data_source":
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_RESOURCE_INVALID",
                "Data Source preflight job has an invalid resource type.",
                409,
            )
        source = session.get(GovernedDataSource, job.resource_id)
        if source is None:
            raise QfError("DATA_SOURCE_NOT_FOUND", "Data Source was not found.", 404)
        if source.state != "ACTIVE" or source.preflight_state != "PENDING":
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_STATE_CONFLICT",
                "Data Source must be ACTIVE with a pending preflight.",
                409,
            )
        config = source.public_config
        if not isinstance(config, dict):
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_CONFIGURATION_INVALID",
                "Data Source does not provide a registered manifest contract.",
                409,
            )
        if config.get("archive_manifest_id") is not None:
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_STATE_CONFLICT",
                "A pending Data Source cannot already pin an archive manifest.",
                409,
            )
        _reject_untrusted_locator_or_secret(config)
        source_spec = config.get("source_spec")
        if not isinstance(source_spec, dict) or source_spec.get("kind") != "plugin":
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_CONNECTOR_UNSUPPORTED",
                "Data Source must provide a registered plugin manifest contract.",
                409,
            )
        binding = config.get("plugin_binding")
        if not isinstance(binding, dict):
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_BINDING_INVALID",
                "Data Source is missing its registered plugin binding.",
                409,
            )
        plugin_binding = _resolve_active_importer_binding(session, binding)
        scopes = source.universe_scope
        if not isinstance(scopes, list) or len(scopes) != 1:
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_UNIVERSE_SCOPE_INVALID",
                "Archive manifest preflight requires exactly one governed Universe Version.",
                409,
            )
        universe_id = _require_uuid(
            scopes[0],
            code="DATA_SOURCE_PREFLIGHT_UNIVERSE_SCOPE_INVALID",
            message="Data Source Universe scope is invalid.",
        )
        universe = session.get(MarketUniverseVersion, universe_id)
        if universe is None or universe.state != "ACTIVE":
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_UNIVERSE_NOT_GOVERNED",
                "Data Source preflight requires an active governed Universe Version.",
                409,
            )
        if not source.provider or not source.license_classification:
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_CONFIGURATION_INVALID",
                "Data Source provider and license classification are required.",
                409,
            )
        return _PreflightContext(
            source_id=source.id,
            universe_id=universe.id,
            manifest_name=f"preflight-{source.id}-{job.id}",
            provider=source.provider,
            source_license=source.license_classification,
            source_spec=source_spec,
            plugin_binding=plugin_binding,
        )


def _verify_descriptor(context: _PreflightContext, descriptor: ArchiveManifestDescriptor) -> None:
    expected_uri = f"manifest://{context.manifest_name}"
    mismatches: list[str] = []
    if descriptor.manifest_uri != expected_uri:
        mismatches.append("manifest_uri")
    if descriptor.provider != context.provider:
        mismatches.append("provider")
    if descriptor.source_license != context.source_license:
        mismatches.append("source_license")
    if descriptor.source_spec != context.source_spec:
        mismatches.append("source_spec")
    if mismatches:
        raise QfError(
            "DATA_SOURCE_PREFLIGHT_DESCRIPTOR_MISMATCH",
            "The remote archive manifest does not match the registered Data Source.",
            502,
            {"fields": mismatches},
        )


def _persist_descriptor(
    factory: SessionFactory,
    context: _PreflightContext,
    descriptor: ArchiveManifestDescriptor,
) -> bool:
    _verify_descriptor(context, descriptor)
    with factory.begin() as session:
        source = session.execute(
            select(GovernedDataSource)
            .where(GovernedDataSource.id == context.source_id)
            .with_for_update()
        ).scalar_one_or_none()
        if source is None or source.state != "ACTIVE" or source.preflight_state != "PENDING":
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_STATE_CONFLICT",
                "Data Source changed while its preflight was running.",
                409,
            )
        config = source.public_config
        if (
            not isinstance(config, dict)
            or config.get("source_spec") != context.source_spec
            or config.get("plugin_binding")
            != {
                "plugin_release_id": context.plugin_binding["plugin_release_id"],
                "plugin_runtime_bundle_id": context.plugin_binding["plugin_runtime_bundle_id"],
            }
        ):
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_CONFIGURATION_CONFLICT",
                "Data Source manifest configuration changed while its preflight was running.",
                409,
            )
        existing = session.scalar(
            select(ArchiveManifest).where(ArchiveManifest.manifest_uri == descriptor.manifest_uri)
        )
        if existing is not None:
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_MANIFEST_CONFLICT",
                "Archive manifest identity is already occupied.",
                409,
            )
        # A manifest is executable evidence only when both remote result
        # classes passed.  A clean probe cannot make a failed/malformed PIT
        # result active or advance the source to READY.
        manifest_active = (
            not descriptor.probe_error_count
            and descriptor.point_in_time_result.get("valid") is True
        )
        manifest = ArchiveManifest(
            manifest_uri=descriptor.manifest_uri,
            data_source_id=source.id,
            universe_version_id=context.universe_id,
            provider=descriptor.provider,
            source_license=descriptor.source_license,
            source_spec=descriptor.source_spec,
            coverage_start=descriptor.coverage_start,
            coverage_end=descriptor.coverage_end,
            scanned_until=descriptor.scanned_until,
            shard_count=descriptor.shard_count,
            total_bytes=descriptor.total_bytes,
            missing_shard_count=descriptor.missing_shard_count,
            probe_error_count=descriptor.probe_error_count,
            schema_revision=descriptor.schema_revision,
            state="ACTIVE" if manifest_active else "INCONCLUSIVE",
            point_in_time_result=descriptor.point_in_time_result,
        )
        session.add(manifest)
        session.flush()
        session.add_all(
            [
                ArchiveManifestShard(
                    manifest_id=manifest.id,
                    shard_key=shard.shard_key,
                    source_url=shard.source_url,
                    coverage_start=shard.coverage_start,
                    coverage_end=shard.coverage_end,
                    size_bytes=shard.size_bytes,
                    state=shard.state,
                    observed_at=shard.observed_at,
                )
                for shard in descriptor.shards
            ]
        )
        if not manifest_active:
            append_event(
                session,
                kind="DATA_SOURCE_PREFLIGHT_INCONCLUSIVE",
                aggregate_type="GOVERNED_DATA_SOURCE",
                aggregate_id=source.id,
                payload={
                    "manifest_id": str(manifest.id),
                    "probe_error_count": descriptor.probe_error_count,
                },
            )
            return False
        updated_config = dict(config)
        updated_config["archive_manifest_id"] = str(manifest.id)
        source.public_config = updated_config
        source.preflight_state = "READY"
        append_event(
            session,
            kind="DATA_SOURCE_PREFLIGHT_COMPLETED",
            aggregate_type="GOVERNED_DATA_SOURCE",
            aggregate_id=source.id,
            payload={
                "manifest_id": str(manifest.id),
                "missing_shard_count": descriptor.missing_shard_count,
            },
        )
        return True


def run_data_source_preflight(settings: Settings, lease: JobLease) -> None:
    """Run a real, isolated manifest scan for one registered Data Source."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        context = _prepare_preflight(factory, lease.job_id)
        descriptor = _runtime().inspect_archive_manifest(
            ArchiveManifestSpec(
                manifest_name=context.manifest_name,
                provider=context.provider,
                source_license=context.source_license,
                source_spec=context.source_spec,
                plugin_id=context.plugin_binding["plugin_id"],
                plugin_version=context.plugin_binding["plugin_version"],
                plugin_bundle_path=context.plugin_binding["plugin_bundle_path"],
            )
        )
        if not _persist_descriptor(factory, context, descriptor):
            raise QfError(
                "DATA_SOURCE_PREFLIGHT_PROBE_ERROR",
                "Archive manifest preflight recorded remote probe errors.",
                502,
            )
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one Data Source archive preflight")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    run_data_source_preflight(
        load_effective_settings(Settings.from_env()),
        JobLease(
            job_id=UUID(args.job_id),
            owner=args.lease_owner,
            attempt=args.lease_attempt,
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
