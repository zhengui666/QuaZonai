from __future__ import annotations

from types import SimpleNamespace

import pytest

import runners.codex_sandbox as codex_sandbox


def test_codex_sandbox_preflight_runs_namespace_probe(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[list[str]] = []

    monkeypatch.setattr(codex_sandbox, "codex_bwrap_path", lambda: "/usr/libexec/bwrap")

    def fake_run(command: list[str], **_: object) -> SimpleNamespace:
        calls.append(command)
        return SimpleNamespace(returncode=0, stderr="")

    monkeypatch.setattr(codex_sandbox.subprocess, "run", fake_run)

    codex_sandbox.codex_sandbox_preflight()

    assert calls
    command = calls[0]
    assert command[0] == "/usr/libexec/bwrap"
    assert "--unshare-user" in command
    assert "--unshare-net" in command
    assert "--proc" in command
    assert command[-1] == "/bin/true"


def test_codex_sandbox_preflight_fails_closed(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(codex_sandbox, "codex_bwrap_path", lambda: "/usr/libexec/bwrap")
    monkeypatch.setattr(
        codex_sandbox.subprocess,
        "run",
        lambda *_args, **_kwargs: SimpleNamespace(
            returncode=1,
            stderr="bwrap: No permissions to create a new namespace",
        ),
    )

    with pytest.raises(RuntimeError, match="No permissions to create a new namespace"):
        codex_sandbox.codex_sandbox_preflight()
