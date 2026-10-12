# Finite research

The core is objective/data → disclosed hypotheses and native trials → validation feedback → selected independent assessment or a recorded stop. Portfolio/backtest and target/account observation are optional consumers, not prerequisites.

```sh
quazonai research models
quazonai datasets
quazonai research start --file original-request.json
quazonai research list
quazonai research get CONVERSATION_ID
quazonai usage
```

A file is a complete `ResearchStart` with an explicit original `requestId`. It may contain `input:{goal,model:{provider,modelId},data,policy}` or the configured-dataset form `{requestId,goal,model,datasetId,maxTrials}`. Native input must match a service-registered dataset; never invent server paths or data. Keep exact bytes and IDs for replay. An optional `--request-id` checks the file's ID; it never replaces it.

For a new objective with an already configured dataset, explicit flags are sufficient:

```sh
quazonai research start --goal 'THE_REQUESTED_OBJECTIVE' \
  --dataset DATASET_ID --model PROVIDER:MODEL_ID \
  --max-trials REQUESTED_LIMIT --request-id ORIGINAL_REQUEST_ID
```

Use model IDs from actual configured availability. Discovery does not run inference, discover Desktop credentials or establish model quality. The host explicitly persists its capability-supported thinking level; hidden reasoning is not evidence to report.

Retain the original policy, budget, source identity and selection/evaluation criteria. Zero trials allows no computation. Committed failures consume attempts and remain disclosed. Missing inputs or unsupported criteria are not permission to change budgets or create another study until something passes.

Use `research get` to inspect all trials, failures, comparison and final/stop reason. Native computation is not a qualification. Final held-out assessment is separate from iterative feedback; never tune against it or rename/copy/fork exposed data to call it unseen. The model-code adapter retains finite fuel, process timeout, owned child recovery and a provider-credential-free execution environment.

After an unknown start response, replay only the same original file or identical explicit flags with the same ID. HTTP 202 means admitted/replayed, not finished. Follow [observation](runs.md) and [recovery](recovery.md), without a replacement key.
