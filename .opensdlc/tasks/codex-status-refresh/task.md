# Codex status refresh

## Goal

Make the local Codex status refresh responsive without inventing model availability, changing native authentication/configuration, or weakening actual Mission execution.

## Diagnosis

The refresh POST called the ordinary native `thread/start` path once for defaults and again for overrides. Ephemeral/read-only threads still loaded native MCP servers and session-start facilities, so a status observation could wait for unrelated tool initialization. Serial native operations had a 110-second overall deadline. The browser then invalidated every Codex query and waited for unrelated profile reads as well.

## Changes

- Add an explicit status-only `probe_thread` path. Read native MCP names through `config/read`, discarding values, and disable those servers and irrelevant startup facilities only in the ephemeral thread request. Retain native default/effective model, provider, effort, tier and complete catalog validation. Ordinary threads and actual Missions retain their own settings.
- Use that path for both native settings inspections. Bound the whole status observation, including home serialization and process cleanup, to 20 seconds. Timeouts remain unavailable outcomes; no stale result is relabeled as fresh.
- After a successful browser probe, refresh and await only the selected profile's observation. Profile edits still invalidate profile data normally.
- Add request-projection unit coverage and a real official App Server regression with a deliberately stalled required MCP. Exercise defaults and a catalog-derived alternative model/effort, asserting no MCP startup, paid turn, authentication file, or native configuration change.

## Verification

Pending on this branch's final Head. Existing GitHub Actions run Rust formatting, Clippy, native tests and browser build/tests. The new `codex_probe` integration test is included by the existing `store-postgres` job's all-server-test command with `server/native-codex`; it requires `CODEX_NATIVE_BIN` pointing to the pinned official binary. Its ten-second deadline detects the deliberately introduced sixty-second MCP stall; it is not a claim about the owner's machine or network latency.

Dedicated real-account/paid-inference checks retain the existing owner waiver. No production service, database, credentials or local installation has been changed. Merge requires applicable current-Head CI and a clean read-only Codex review.
