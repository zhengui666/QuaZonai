# Remote Nautilus instances

Owner request: branch from remote `dev`, remove embedded Nautilus execution from the application, configure separate remote instances for backtests, paper and live observations, then merge the verified change into remote `dev`.

Base: `0291f0992389b82a2e5f9c17ace36a2ee9b58655`; branch: `codex/remote-nautilus-instances-20261025`.

## Implementation

The application image contains no native job or source-tools executable. `nautilus.backtest` selects a remote scientific worker for research, heldout evaluation, portfolio backtests, capacity and PM calculations. Existing `connections` retain independent paper/live URLs, accounts, credential references and captured observation bindings. The separate Nautilus worker image runs the original scientific job through a versioned HTTP adapter; the adapter does not implement matching, accounting or broker/order execution.

New research inputs and scientific child tasks capture their remote binding. Model, scope and UUID object artifacts travel with each request; catalog data and the application's frozen catalog copies require shared paths mounted read-only in the worker. Existing PIT, source revision, frozen input, sealed evaluation, numerical and trial-budget checks remain. Configuration changes do not rewrite stored requests or observation connections. Old pending local-executable tasks fail explicitly instead of silently executing locally or moving to a different instance.

Each scientific invocation has its own identity. A lost POST response is reconciled through that invocation's receipt, and a cancellation is acknowledged only after the worker joins the child process. An abandoned RUNNING receipt becomes UNKNOWN on restart. These are pure scientific commands; replay of the original durable safe task retains its original trial accounting. Downstream target ACKs remain distinct from actual fills.

## Verification and delivery

Applicable checks: TypeScript/service/research/CLI suites; independent worker HTTP/process tests; Web build/tests; deployment/package contracts; actual native local/HTTP result comparisons; existing retained science and core flow; final-Head GitHub CI and independent review. Controlled software protocols do not establish market quality, profitability, real-account execution or a user deployment.

Verification is in progress. No merge, release or production deployment is claimed by this task record.
