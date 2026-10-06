# Offline scientific artifact contract discovery

## Goal

An external service Agent can discover scientific artifact content through the
installed CLI and portable service Skill, without a repository or private runbook.

## Scope

Add `openapi --domain` to both portable CLI and server-compatible entrypoints.
Reuse the existing native domain export and focused reference closure. Keep the
HTTP default, DTOs, scientific validation, authority and generated files unchanged.
Teach the Skill to select the appropriate outer request and inner artifact DTO.

## Acceptance

- Domain list, full export and named scientific DTO selection run offline
- Selected schemas equal native Rust exports and retain all transitive references
- Strict fields, numeric V1 and string counters agree with actual DTO parsing
- Unknown names, unsupported version names and conflicting selectors fail safely
- Documentation commands execute; existing CLI/Skill checks remain green

These checks establish discovery and wire behavior, not real Agent performance,
scientific qualification, runtime execution or deployed-server compatibility.

Exact-head CI, version publication and deployed compatibility remain separate evidence.
