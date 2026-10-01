# Protect detached Settings work on browser leave

## Scope and behavior

The authenticated console previously registered its browser-unload warning only
for explicit editor guards. A Settings command could retain a lost-response
request and its idempotency identity after its dialog closed, while refresh
silently discarded that in-memory recovery state.

[GuardProvider](../../../apps/web/src/ui.tsx) now observes the existing acyclic
[Settings work registry](../../../apps/web/src/settings-work.ts) for document
leave only. Either explicit guarded work or active Settings work installs the
existing `beforeunload` listener. The handler checks current work, and the effect
removes the listener once both sources clear. `GuardContext.blocked` still
contains only explicit guards: Settings category changes, main navigation and
supported autosave detachment remain available. Pending explicit dialogs retain
their existing disabled Close/Cancel and no-op Escape behavior.

The existing Settings registry continues to own the lifecycle of pending or
uncertain commands, unacknowledged receipts, errors and autosave sessions. This
change does not add storage, credentials serialization, automatic replay,
request cancellation, URL/history routing, a new guard framework or backend
behavior. PWA update protection and the error-page reload confirmation remain
independent and unchanged.

## Lifetime and recovery limits

Recovery remains memory-only within the current document. Choosing to leave can
discard inputs, retry identities and receipts; startup reads current state and
does not replay the old command. `beforeunload` is browser-controlled and cannot
guarantee a warning on forced termination, crashes or mobile process eviction.

`GuardProvider` remains inside `AuthGate`. This patch protects the mounted
authenticated console, including closed Settings dialogs and other sections;
it does not extend protection past provider unmount on session expiry or a root
crash. It does not provide durable reload or session recovery.

## Verification and delivery gates

Implementation base: `15c626f77abf88418fb5019590edd6b5512ec190` on dev. Only this
task, `ui.tsx`, and the existing
[Settings browser suite](../../../apps/web/tests/settings-autosave.spec.ts)
change. No contract or generated output changes are intended.

The browser cases use real Playwright reloads after normal user activation and
assert native `beforeunload` dialogs, rather than synthetic event dispatch:

- Lost response, dialog close, Settings/main navigation, Stay, and explicit retry
  preserve the exact method, path, complete body and nonempty idempotency key
- Staying sends no duplicate mutation; a confirmed receipt survives a failed
  list refresh and still protects reload until acknowledged
- A detached definitive error protects reload until its error is acknowledged
- Two detached pending autosaves allow in-app navigation, retain protection
  after one settles, and remove the warning once both finish without extra writes
- An ordinary dirty project retains its browser warning, input and explicit
  discard confirmation; confirmed discard permits a clean reload
- Pending data-source registration cannot close on Return/Escape; accepting a
  later warning with an uncertain result reloads without replay or fake success
- Clean reload completes without any native dialog after the relevant work
  clears; the existing PWA and error-page cases remain in the applicable gates

Local `make check-web` passed: unchanged generated contracts, 42 validator/loader
tests, TypeScript, 579 unit tests, the production/PWA build and the modular build
size gate. Browser test discovery succeeds (72 tests in the existing auth UI
configuration); discovery is not execution. No local
browser/server startup was attempted because this executor's socket restriction
and absent native Docker prerequisites are already established. Hosted
`test:auth-ui` and the existing real Rust/PostgreSQL/Caddy browser/PWA gate are
required before merge. `make check-links` is blocked by the missing pinned
`lychee` executable; no documentation check pass is claimed.

Publication has not occurred. The exact final published Head still requires all
applicable CI and independent native review under [review](../../review.md).
Resolve findings and verify that exact Head before merging into dev; local
checks and test discovery do not replace those gates.
