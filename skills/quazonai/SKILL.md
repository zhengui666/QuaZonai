---
name: quazonai
description: Operate an existing QuaZonai service through its native CLI. Submit and observe durable factor research, inspect original data and evidence, use optional scientific/portfolio/target workflows, and recover uncertain operations. Not for implementing, building, testing or deploying QuaZonai.
compatibility: Requires the installed quazonai executable and an owner login or an already provisioned scoped connection for supported non-research operations.
---

# Operate QuaZonai

Operate within the user's requested scope and report actual observations. This directory needs no repository checkout. Development, service startup and migrations are outside this Skill.

## Connect and choose the operation

Use installed `quazonai client` and native request contracts. Read [connection and discovery](references/connection.md) on first use, when the connection changes, or on an identity error. Reuse the saved device connection. If no login exists, have the user run `quazonai client login` in their own terminal and enter the frontend address and hidden password there. Never ask for the password in chat or read the saved profile/token yourself.

New AI factor research uses `research` commands backed by official Pi Durable, not the retired Codex/Mission MCP surface. Research views/start/stop require current owner authority. Scoped scientific/Downstream credentials keep their own permissions; never replace one with owner login to bypass a denial, self-issue grants, or change criteria to force a pass.

| Intent | Procedure |
| --- | --- |
| Submit a research objective; inspect data/policy, trials, native tasks and actual usage | [Research](references/research.md) |
| Follow an optional scientific Run, observe events or cancel it | [Runs](references/runs.md) |
| Explain original Alpha/portfolio/target evidence and delivery | [Results](references/results.md) |
| Inspect original account sources or relay already-produced native observations | [Account observations and transport](references/account-transport.md) |
| Control a requested partial managed-capital exit or inspect cash | [Capital exits](references/capital-exits.md) |
| Unknown outcome, conflict, failure or exhausted budget | [Recovery](references/recovery.md) |

Discover unfamiliar fields with the exact command's `--help`, `quazonai openapi --schema NAME` for HTTP inputs or `quazonai openapi --domain --schema NAME` for scientific content. Report unsupported capabilities rather than inventing routes or installing an alternate engine. Optional portfolios and target delivery are not prerequisites for factor research.

## Execute and return a receipt

1. Reuse original resource IDs. Native durable Conversation/Submission/Task IDs are numbers; existing scientific IDs remain UUIDs and big counters/clock values remain decimal strings. Do not interchange them.
2. Preserve original goal, dataset identity and evaluation/selection/stop criteria. Inspect real source/PIT/coverage/permission limitations; a source name or existing path proves none of them.
3. For research, retain the exact submission file and `requestId`; same-body replay returns native identities. Existing scientific writes retain their original idempotency key and expected revision. Preview validates local syntax, not authority or scientific eligibility.
4. Submit once and inspect the original persistent state. HTTP 202 is admission/stop requested, not execution or cancellation proof. Do not treat stopped observation as stopped work; use [recovery](references/recovery.md) after uncertain writes.
5. Stop at requested scope or an actual unresolved boundary. Return operation → IDs → observed state → evidence/limitations → next action. Distinguish failed, running, unknown, not attempted and computed-but-unqualified.

Keep failed/rejected trials and disclose the compared set and selection. Never tune against final held-out evidence, relabel exposed samples as unseen, invent missing metrics/values, or claim success from an exit code. Provider credentials stay in trusted host configuration, not requests or conversations. Never expose credentials, hidden reasoning, raw Sealed values or unrequested artifact contents.

Target-only delivery creates no real broker orders or real-account execution. Optional capital-exit control uses only its already-supported native owner boundary and never transfers or withdraws funds.
