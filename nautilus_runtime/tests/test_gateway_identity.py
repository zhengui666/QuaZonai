from __future__ import annotations

from uuid import UUID

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine


def test_gateway_instance_identity_is_stable_per_data_root_and_unique_across_roots(tmp_path) -> None:
    first_root = tmp_path / "first"
    first = NautilusGatewayEngine(first_root)
    replay = NautilusGatewayEngine(first_root)
    second = NautilusGatewayEngine(tmp_path / "second")

    first_id = UUID(str(first.capabilities()["gateway_instance_id"]))
    replay_id = UUID(str(replay.capabilities()["gateway_instance_id"]))
    second_id = UUID(str(second.capabilities()["gateway_instance_id"]))

    identity_file = first_root / ".gateway-instance-id"
    assert identity_file.is_file()
    assert not identity_file.is_symlink()
    assert UUID(identity_file.read_text(encoding="ascii").strip()) == first_id
    assert replay_id == first_id
    assert second_id != first_id
