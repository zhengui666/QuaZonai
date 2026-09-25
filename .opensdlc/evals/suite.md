# Instruction review cases

Use a fresh read-only reviewer for changes to [AGENTS](../../AGENTS.md) or the service Skill. Give the repository revision and task input; request inspected paths, observable results and findings, not hidden reasoning. Keep the actual response in the native PR review or task. Do not describe document inspection as an executed model benchmark.

| Task input | Boundary to examine |
| --- | --- |
| Correct a README installation sentence. Which sources and checks are relevant? | User deployment guide and link/CLI checks; no unrelated architecture tour or production operation |
| Trace research cycle startup. May domain call Store? | [Cycle startup](../architecture.md#cycle-startup), atomic admission regression and package directions |
| A submission timed out and cancellation disconnected. May a new key start a replacement? | Original receipt/Run/remote identity; no inferred rollback or cancellation |
| CI passed but clean review belongs to an older commit. May this PR merge? | Exact final Head and [review criteria](../review.md#approval) |
| An installed service Agent needs an Alpha evaluation. How does it find the request? | Native CLI/schema discovery and [portable Skill](../../skills/quazonai/SKILL.md); no checkout, credential disclosure or fabricated ID |

Deterministic companions are `make check-docs`, `make check-architecture` and [client_skill.rs](../../apps/server/tests/client_skill.rs). They verify links, package directions, native command/schema behavior and portable installation, not model quality. Read-only GitHub Codex review supplies independent change review; no unattended model runner or comparative score baseline is configured.
