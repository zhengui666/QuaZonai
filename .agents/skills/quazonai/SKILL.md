---
name: quazonai
description: Operate the QuaZonai quantitative research workbench through the quazonai CLI, translating user goals into commands and reading JSON receipts. Use for datasets and models, research submission and continuation, task tracking, official native strategy backtests, parameter grids and evidence assessments, paper/live account observation and explicit control of configured strategies, capital exits, usage, settings, extensions, and maintenance. For agents using an installed product on the user's behalf, not editing its source, implementing features, or deploying its backend.
---

# Quazonai

Translate user goals into native `quazonai` commands, retain requests and receipts, and report actual business outcomes. Use the CLI rather than accessing the service database or constructing a separate HTTP client.

## Responsibilities

This skill is for external agents: submit goals to QuaZonai, read the same research's state, conversation, native results and assessments, and deliver them to the user. The CLI is the operating and reading interface to the same service. The internal Pi Durable research agent uses its own strategy-authoring skill and tools to implement source, build, run, and evaluate research. Collaborate through research goals or continuation instructions; do not replace internal execution or write strategy source under this operating skill.

## Release alignment and response language

Use the skill shipped with the same release as the service and CLI. Do not apply a moving `dev` skill to an older instance. Follow release instructions for installation and upgrades; this skill does not automatically install other agent products.

Read `quazonai settings get` before composing normal responses. Follow its `language` setting, shared with the web settings: `en` (default) or `zh-CN`. An explicit language request from the user for the current exchange takes precedence. Keep CLI names, API fields, identifiers, and diagnostic strings unchanged. This English skill does not itself require English user-facing responses or change the saved language.

## 1. Connect and identify available capabilities

Start with:

```sh
quazonai --version
quazonai --help
quazonai identity
quazonai settings get
```

Reuse the current login. For a missing CLI, initial login, instance switch, or certificate issue, read [Connection, output, and errors](references/connection-and-errors.md). The user must enter the initial login password in their own interactive terminal.

Choose commands from the installed version's `--help` and `schema`. Do not add `--json`, `--yes`, `--api-key`, `--password`, `--profile`, or an arbitrary HTTP request subcommand: the current CLI has none of these interfaces.

## 2. Choose the shortest operating path

| User goal | Read first | Reference |
| --- | --- | --- |
| Natural-language research, backtests, questions, or continuation | `datasets`, `research models`, `research get ID` | [Native research and grids](references/research-and-portfolios.md) |
| Custom strategy research | `research start` / `research resume`, specifying goals and constraints | [Research and evidence](references/research-and-portfolios.md) |
| Parameter grids and evidence assessment | Original research detail, registered datasets and runner capabilities, `research native-backtests ID` | [Native research and grids](references/research-and-portfolios.md) |
| Account binding, explicit native strategy start/stop, or capital exit | `observation list`, `account-strategy status ID`, `capital status ID` | [Accounts and capital](references/accounts.md) |
| Existing PM/portfolio records | Actual `--help`, `pm capabilities` | [Retained historical queries](references/pm.md) |
| Usage, settings, extensions, or maintenance | Corresponding `get`, `list`, `status` | [Command index](references/commands.md) |

Independent research does not require a PM, portfolio, or account observation. Continue with original IDs when related resources already exist; do not resubmit the same work.

## 3. Form the request

1. Obtain dataset, model, and business IDs from actual lists, details, or receipts. Preserve original IDs. Only research `conversationId` and `submissionId` are native safe integers; pass other IDs as opaque strings.
2. For file requests, read `quazonai schema list`, then `quazonai schema show NAME`. Native science contracts use `--domain`; do not mix the two name sets.
3. Ordinary `schema show` returns `{format:"typescript", entry, source}`; `source` contains multiple contracts. Locate the target type and dependencies rather than submitting the entire output. Native `--domain` returns JSON Schema and referenced components. Schemas describe structure, not account permissions, data availability, or scientific validity.
4. Write one JSON object containing the user goal, actual resources, and explicit parameters. Ask only when a required goal, amount, connection, or authorization scope cannot be determined from current evidence. Do not invent models, connections, amounts, or constraints.
5. Generate and retain one unique string for each new operation requiring `requestId`. When reusing a request, preserve its file contents, `requestId`, resource IDs, and `expectedRevision`. If supplied, `--request-id` must equal the value in the file.
6. Keep monetary amounts, weights, nanoseconds, and decimal fields as strings, such as `"10000"` and `"0.25"`. Preserve `null` and missing values; do not replace unavailable metrics with zero.

Write files with a JSON serializer. Pass text and paths through argument arrays or correct shell quoting; never turn user text into shell code. Prefer `--file` for large requests, not nonexistent inline JSON options.

## 4. Submit once and observe the original object

```sh
quazonai research start --file research-request.json
quazonai research wait "$research_id" --timeout 60
quazonai research get "$research_id"
```

Get `research_id` from the first receipt, not an example. Retain stdout JSON, stderr, and exit status separately.

- Ordinary success usually produces one JSON line; `watch` emits multiple state JSON lines followed by a summary. `--help` and `--version` are text.
- Without `--timeout`, `watch` / `wait` keep waiting. Specify seconds for bounded observation. Timeout or interruption only detaches the client, not the remote work.
- Research ends when its original submission is `done` / `unanswered`. `done` still requires inspection of native results, evidence completeness, metrics, and errors; it does not establish the scientific goal.
- Portfolios settle at `COMPUTED` / `FAILED` / `STOPPED`. Exit code `0` alone does not establish completion of the observed object.
- On `write_outcome_unknown_retain_original_request`, query the original object or list first. Without a receipt, retain the unknown outcome. Account strategy commands only query the original requestId and are not automatically resent; other operations follow their explicit idempotency contracts.
- On revision conflict, read current details and reassess the original intent. If starting a new operation, save a new request with a new `requestId`. Never silently change the revision of an original request whose outcome is unknown.

See [Connection, output, and errors](references/connection-and-errors.md) for exit codes and recovery.

## 5. Report actual business meaning

Report the instance, resource IDs, current state, key results, blockers, and request/receipt file locations. For unfinished work, include the next command to read the original object.

Distinguish historical backtests, AI suggestions, and account facts. paper/live observation only binds an existing account. Completing research does not start an account strategy; a strategy start receipt is not a fill, and acceptance of a capital exit command is not receipt of funds.

Stop, delete, and control accounts only within the user's authorized scope. `delete` hides eligible inactive objects while retaining evidence; it neither stops remote work nor liquidates positions. Report active-state or dependency blockers rather than additionally stopping or deleting related resources on your own.
