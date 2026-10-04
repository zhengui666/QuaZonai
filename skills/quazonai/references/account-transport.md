# Relay an existing native account source

This transports already-produced official Nautilus 0.63.0 snapshots or Q account envelopes. It does not launch a native node, subscribe to a venue, create credentials, send orders, or establish that a Paper/Live account is connected. Use only the existing authorized Downstream connection and its configured project/environment/account/session.

## Retained native snapshots to envelopes

If the native host already exports official `PortfolioSnapshot` JSON, retain that original input. Each stream record must be one complete JSON object followed by a newline. On the machine with the native converter installed:

```sh
job native-account-observation --binding /private/native-binding.json \
  --last-sequence 0 --dropped-events 0 \
  --stream --output /private/observations-001.ndjson \
  < /private/native-snapshots.ndjson
```

Use sequence zero only for a new observer session. The converter creates a new file and syncs each envelope before reading another native snapshot; an existing output path fails without overwriting it. It preserves native amounts, clocks and stale/unpriced flags, and always records connection `UNKNOWN`. A host-owned snapshot stream can supply stdin continuously; keep the original input separately for recovery. Run the converter outside the native event thread. Never block that thread on a pipe, disk sync or HTTP request.

If the native host uses Q's bounded observer queue, its separate consumer can instead append the original `AccountObservationSubmitV1` envelopes as NDJSON. Preserve their sequence, dropped-event count, connection state and clocks unchanged. The observer queue is not durable; queue losses remain explicit gaps. This transport does not install that host integration for you.

## Relay and recover

The portable CLI reads retained envelope NDJSON through the existing authenticated account intake:

```sh
quazonai client --origin https://qz.example \
  --credential-file /private/qz-downstream-token \
  forward accounts relay --input /private/observations-001.ndjson
```

Add `--follow` to wait for appended records after the file exists. Omit it to finish at the current EOF. Each line must fit within 2 MiB including its newline. Stdout emits the validated native intake receipts as NDJSON; keep receipts private along with the account data. The relay reads no next record until the previous receipt echoes the submitted envelope.

The default is at most five attempts per record, one second apart, for an unavailable/unknown HTTP result or an explicitly retryable server Problem. `--max-attempts 1` disables automatic retry. Rejections, mismatched receipts, output failure or exhausted attempts stop the relay and leave the input untouched. No idempotency header is needed: the server atomically identifies replay by the native source binding, sequence/event identity and complete envelope. Do not supply a global `--idempotency-key` or regenerate an envelope after unknown delivery. Other CLI write commands retain their existing idempotency-key requirement.

Restart the same command with the same retained file. It replays from the beginning and relies on the server's existing immutable source/session/event/sequence replay receipts. It never refreshes heartbeat or valuation time on replay. There is no separate local cursor file to lose. A finite `--preview` validates and prints redacted request plans without contacting the server; preview cannot be combined with follow.

Follow waits at an incomplete final line; finite mode reports `CLI_ACCOUNT_STREAM_INCOMPLETE_RETAIN_INPUT`. A detected file replacement, disappearance or truncation reports `CLI_ACCOUNT_STREAM_CHANGED_REPLAY_OR_NEW_SESSION_REQUIRED`. Keep segments append-only and never rotate or rewrite a path while following it. Retain the raw native input and all segments; the relay neither compacts nor deletes them. It does not detect arbitrary in-place rewrites that keep the same file identity and length, or guarantee survival beyond the filesystem's durability guarantees.

To continue a still-running native session in a new segment, explicitly supply its known sequence/drop cursor and use a new output path. Replay existing envelopes before converting more input; reconversion would give them new source observation clocks. A native-node restart or lost producer cursor requires a new native session binding and disclosure of the break. Missing native history cannot be recreated by this transport.

Read back with `forward accounts sources PROJECT_ID`, then `forward accounts current PROJECT_ID SOURCE_ID` and `forward accounts history PROJECT_ID SOURCE_ID`. Preserve separate currencies, connection/valuation freshness and gaps. Account totals can span strategies; they do not establish a release's profit or full broker lifetime history. A synthetic stream test is not real-time Sandbox or Live acceptance.
