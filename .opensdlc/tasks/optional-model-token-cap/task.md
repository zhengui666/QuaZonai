# Optional application model-token cap

## Scope

Represent an absent application token budget explicitly as `None`, including
turn reservation, persistence, native dispatch, accounting and exact replay.
Do not turn absence into an `i64::MAX` reservation. This change does not alter
provider context windows, scientific admission, credentials, leases, cancellation,
cost enforcement or existing bounded-token behavior.

## Implementation

- `ModelReservation.tokens`, `TurnRequest.tokens` and stored `Reservation.tokens`
  are optional; an uncapped request adds no token reservation
- A finite frozen cycle cap cannot admit an uncapped turn
- Initial, feedback and review turns share the optional remaining-cap calculation
- PostgreSQL retains `NULL` for an uncapped unresolved reservation and retains
  its outstanding turn count; non-null caps must remain positive
- Native threshold stops and known token-overrun checks require a finite cap
- Actual usage remains a required, immutable native receipt; unknown usage
  cannot refund spending, permit a replacement turn, or acknowledge the queue
- Exact replay distinguishes `None` from every `Some` value, including `i64::MAX`

## Verification status

Candidate only. Rust compilation, formatting, Domain tests, PostgreSQL/PGMQ tests
and native-Codex tests have not run: this authoring environment has no Cargo,
rustc, rustfmt or psql. No remote branch, service configuration or deployment was
changed. Static inspection is not a passing test result.

Added coverage comprises four Domain cases, three Store-local cap-calculation
cases, seven Store transaction cases and one native Worker case. Existing bounded
tests retain their meaning using explicit `Some` reservations.

Run on the final integrated candidate:

```sh
cargo check --locked --workspace --all-targets --all-features
cargo test --locked -p domain --test rules
cargo test --locked -p store --lib token_cap_tests
cargo test --locked -p store --test optional_token_caps --test turns --test missions --test turn_recovery --test terminals
cargo test --locked -p server --features native-codex --test mission_worker
```

Store/Server cases require the repository's disposable PostgreSQL/PGMQ and native
system prerequisites. Integration with the separately authored optional-deadline
change requires final-Head compilation and regression before any delivery claim.
