# Bounded observation and stop

Before watching, identify the requested endpoint: a status check, an admission receipt, or completion of the original study with its scientific conclusion. Reuse the accepted numeric `conversationId` and `submissionId`; if only a name is supplied, resolve it with `research list` and inspect the matching input, model and dataset. Do not choose the newest study merely because its title is similar.

Use the original numeric `conversationId` from the research admission receipt:

```sh
quazonai research get CONVERSATION_ID
quazonai research watch CONVERSATION_ID --timeout 30
quazonai research wait CONVERSATION_ID --timeout 30
```

Timeout units are seconds; there is no default observation deadline. Watch emits NDJSON snapshots and a final observation result. Wait returns exit0 for native `done`, exit2 for `unanswered`, or exit124 when its explicit observation budget ends before settlement. Done means submission settlement, not scientific qualification. A bounded watch ending says nothing about research completion.

| Observation | Next agent action |
| --- | --- |
| Latest `submission` is present but not `done`/`unanswered` | Continue bounded observation when completion is requested; report its actual status. A chat response boundary is not a reason to stop the study. |
| `submission.status: done` | Read `final`, `criterion` and `trialRecords`; report the selected assessment or recorded stop. Do not infer scientific success from exit0. |
| `submission.status: unanswered` | Read `research diagnostics ID`, preserve the submission reason and existing trials, then apply [recovery](recovery.md). |
| Missing submission, timeout or detachment | Report settlement as unknown/not yet observed, retain the last snapshot, and read the same ID again. Do not start replacement work. |

Treat submission settlement, the summary `status` and scientific `final.status` as separate facts. A `COMPUTED` final can miss the requested criterion; `STOPPED` can have no selected trial. Use [results](results.md) to answer the original question.

The service projects the latest native input submission and separately retains `originalSubmission`. An explicit owner-authorized recovery uses a new normal input in the same conversation; it never reopens or overwrites the original failed submission/task. If native projection is unavailable, the CLI reports `nativeSubmissionAvailable:false` and cannot infer settlement from IDLE, a final record or empty tasks. Use the [recovery procedure](recovery.md), not an invented route or a replacement study.

Ctrl-C/detachment stops only observation. It does not revoke the original admission, reset budgets or stop scientific work. Read the same research ID again when requested; polling is not a gap-free durable event history.

For a continuing task, explicitly choose the next bounded observation after each window; do not silently turn a 30-second timeout into a failed study or an abandoned completion request. There is no watch resume cursor: `research list --cursor` is list pagination, not an event offset.

Only an explicit request to stop work uses:

```sh
quazonai research stop CONVERSATION_ID
quazonai research get CONVERSATION_ID
```

The host invokes native abort for owned work and children. A stop receipt or local process exit still cannot prove an external provider/downstream operation cancelled. Reconcile the original native state and retain unknown/late remote outcomes. Service shutdown suspends native work for reopen; it is not a cancel operation.

When handing work to another agent, retain the service identity, original request file/`requestId`, `conversationId`, original and latest `submissionId`, last snapshot time/status, trial/final evidence and requested completion condition. State whether stop or resume was requested and include its receipt or unknown outcome. The recipient continues the same study and reconciles the same operation; a new chat does not justify a new research request. Never include credentials in the handoff.
