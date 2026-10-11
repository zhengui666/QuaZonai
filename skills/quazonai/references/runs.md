# Observe and reconcile Runs

## Find the original Run

Use a Run ID from the original accepted command receipt or `cycle show`. If the user supplied only a project, use `run list --project-id PROJECT_ID --limit 20` with the saved login (or the original scoped connection flags) and resolve the intended Run from returned metadata. `run show RUN_ID` reads its actual state and active Attempt. Do not start another Cycle or switch identities to get a more convenient result.

## Bounded event observation

```sh
quazonai client run watch "$RUN_ID" --max-seconds 30 --max-events 100
```

The time and event limits above are explicit observation budgets. Both flags are
optional; omitting one leaves that dimension uncapped. Omit both only when the
user has requested ongoing observation.

Add `--development-http` only for an explicitly configured HTTP connection. After a bounded observation, persist the last verified event ID and explicitly resume when the task still requires it:

```sh
quazonai client run watch "$RUN_ID" --after "$LAST_EVENT_ID" --max-seconds 30 --max-events 100
```

The cursor is the same Run's `UUIDv7:decimal-sequence`. Keep it verbatim. NDJSON events contain `schema_version`, `event_id` and `event`; the final observation summary contains `watch_ended`, `last_event_id`, `events_received` and `cancellation_requested=false`. That summary is the end of observation, not a terminal business result. After a relevant transition or a stream ending, read `run show` to report the current snapshot.

The CLI does not automatically reconnect. Bounded completion, Ctrl-C and a broken stream never cancel the server Run. Report “still running; observed through cursor …” when appropriate. Respect response limits; do not follow indefinitely without an explicit ongoing task. For an incompatible event or cursor reset, use [recovery](recovery.md); do not fabricate a continuous event history.

## Cancellation is a separate write

Only an explicit cancellation request with current authority may use `run cancel RUN_ID`, native `RunCancelV1`, its original key and required authorization. Durable research instead uses `research stop CONVERSATION_ID`; neither stops work merely by closing an observer.

A cancellation request receipt does not prove the remote process stopped. Continue reconciling the same Run and report the server's observed state. Do not kill host processes or invoke downstream exchange controls. Unknown or late remote outcomes remain unknown until the service reconciles them.

## Completion statement

Use actual Run and Attempt IDs, the observed state, observation time/cursor and returned outputs/failure information. Successful execution is not automatically scientific PASS; read the evaluation and qualification records needed for the user's question in [results](results.md).
