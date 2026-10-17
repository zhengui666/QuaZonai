# Bounded observation and stop

Use the original numeric `conversationId` from the research admission receipt:

```sh
quazonai research get CONVERSATION_ID
quazonai research watch CONVERSATION_ID --timeout 30
quazonai research wait CONVERSATION_ID --timeout 30
```

Timeout units are seconds; there is no default observation deadline. Watch emits NDJSON snapshots and a final observation result. Wait returns exit0 for native `done`, exit2 for `unanswered`, or exit124 when its explicit observation budget ends before settlement. Done means submission settlement, not scientific qualification. A bounded watch ending says nothing about research completion.

The service projects the latest native input submission and separately retains `originalSubmission`. An explicit owner-authorized recovery uses a new normal input in the same conversation; it never reopens or overwrites the original failed submission/task. If native projection is unavailable, the CLI reports `nativeSubmissionAvailable:false` and cannot infer settlement from IDLE, a final record or empty tasks. Use the [recovery procedure](recovery.md), not an invented route or a replacement study.

Ctrl-C/detachment stops only observation. It does not revoke the original admission, reset budgets or stop scientific work. Read the same research ID again when requested; polling is not a gap-free durable event history.

Only an explicit request to stop work uses:

```sh
quazonai research stop CONVERSATION_ID
quazonai research get CONVERSATION_ID
```

The host invokes native abort for owned work and children. A stop receipt or local process exit still cannot prove an external provider/downstream operation cancelled. Reconcile the original native state and retain unknown/late remote outcomes. Service shutdown suspends native work for reopen; it is not a cancel operation.
