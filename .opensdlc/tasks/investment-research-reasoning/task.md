# Investment and research reasoning controls

**Source:** The owner requested configurable reasoning effort and speed for investment decisions and research, developed on a new branch from remote `dev` and merged back without review. This explicit instruction overrides the repository's review requirement for this task; applicable verification still runs.

**Base:** `555dc97848cc6f76930e52f27279993346c65fa2`.  
**Branch:** `feat/investment-research-reasoning`.

## Behavior and acceptance

- Investment intake accepts optional `thinkingLevel` and `serviceTier` in `PmMandateStart`, also available through the CLI's existing `pm start --file` and source-derived schema.
- Explicit choices override matching default-model preferences. Otherwise the selected model's effective reasoning effort is frozen at admission; another model's preferences never leak across models.
- Admission rejects unavailable models and unsupported effort/speed before creating a mandate. The original command stays separate from resolved settings so retries retain their original request identity after preference changes or service restart.
- Every mandate research campaign passes the frozen choices to official Durable research execution and the existing provider adapter. Existing records without these optional fields remain readable.
- Investment intake, independent research and model settings expose reasoning effort and speed. Supported models offer standard/Fast; other models retain a disabled speed field with a capability explanation. Model changes clear unsupported options. Draft reload and uncertain retries preserve their original values.
- Settings clearly apply to both investment decisions and research. This change does not alter scientific trial budgets or investment qualification.

## Verification

`make check-web` passes types, 26 unit tests and production build. Focused service typecheck and 11 HTTP/Durable/model/PM tests pass, including the three new `pm-reasoning.test.ts` cases. Full TypeScript/deployment-helper and 59-case browser regression checks run before delivery; final CI and merge receipts belong to the PR and project board.

The browser checks exercise production UI behavior, real HTTP/Durable paths and controlled transports. Service tests use official Faux Provider; they establish execution configuration, persistence and replay, not paid-model quality or a measured inference latency improvement. Standard/Fast availability remains determined by the selected model provider. Merge is separate from release or installed-service deployment.
