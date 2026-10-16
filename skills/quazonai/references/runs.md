# Bounded observation and stop

Use the original numeric `conversationId` from the research admission receipt:

```sh
quazonai research get CONVERSATION_ID
quazonai research watch CONVERSATION_ID --timeout 30
quazonai research wait CONVERSATION_ID --timeout 30
```

Timeout units are seconds; the default is 30. Watch emits NDJSON snapshots and a final observation result. Wait returns a result and exit 0 only for native submission `done`, exit 2 for native `unanswered`, or exit 124 when its observation budget ends before settlement. Done means the native submission settled, not that scientific evidence passed. A bounded watch ending normally says nothing about research completion.

The service must project the original native submission status. If that projection is unavailable, the CLI reports `nativeSubmissionAvailable:false` and cannot declare settlement from IDLE, a final record, HTTP 202 or empty tasks. Report the service capability gap instead of inventing an endpoint or another status store.

Ctrl-C/detachment stops only observation. It does not revoke the original admission, reset budgets or stop scientific work. Read the same research ID again when requested; polling is not a gap-free durable event history.

Only an explicit request to stop work uses:

```sh
quazonai research stop CONVERSATION_ID
quazonai research get CONVERSATION_ID
```

The host invokes native abort for owned work and children. A stop receipt or local process exit still cannot prove an external provider/downstream operation cancelled. Reconcile the original native state and retain unknown/late remote outcomes. Service shutdown suspends native work for reopen; it is not a cancel operation.
