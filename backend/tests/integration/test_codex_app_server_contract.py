from __future__ import annotations

import openai_codex
from pathlib import Path
from dataclasses import replace

from settings import Settings
from runners.research_missions import _codex_service_tier, _codex_thread_config


def test_runtime_controls_round_trip_through_pinned_codex_app_server(
    settings: Settings,
    tmp_path: Path,
) -> None:
    from openai_codex import ApprovalMode, Codex, CodexConfig, Sandbox

    assert openai_codex.__version__ == "0.144.4"
    cases = (
        (
            replace(settings, codex_reasoning_effort="high", codex_fast_mode=True),
            "fast",
        ),
        (
            replace(settings, codex_reasoning_effort=None, codex_fast_mode=False),
            None,
        ),
    )

    codex_home = tmp_path / "codex-home"
    codex_home.mkdir()
    with Codex(
        CodexConfig(
            env={
                "CODEX_HOME": str(codex_home),
                "RUST_LOG": "error",
            }
        )
    ) as codex:
        for configured, expected_service_tier in cases:
            thread = codex.thread_start(
                approval_mode=ApprovalMode.deny_all,
                ephemeral=True,
                cwd=str(tmp_path),
                model=configured.codex_model,
                service_tier=_codex_service_tier(configured),
                sandbox=Sandbox.workspace_write,
                config=_codex_thread_config(configured),
            )
            assert thread.id
            assert _codex_service_tier(configured) == expected_service_tier
