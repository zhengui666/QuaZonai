# Data editor dismissal and retry recovery

## Observed gap and scope

On dev, grant, revocation and native-version registration dialogs closed directly
on Cancel/Escape, dropping both edited inputs and their in-memory idempotency
intent. Source editing warned only for dirty fields, so an unchanged submission
with an unknown transport result could also lose its retry identity silently.

Use one small shared dismissal hook for the four data mutation editors:

- Pending writes cannot be dismissed; the Cancel button is visibly disabled
- Pristine unsubmitted forms close without a confirmation
- Dirty forms require an explicit decision; continuing preserves all inputs
- Failed attempts warn that closing does not cancel a sent request and destroys
  the current retry identity. Continuing and retrying unchanged input retains
  the original existing `Intent` key/body
- Deduplicate an already-open confirmation; retain native modal keyboard behavior

This is presentation/recovery behavior only. It does not cancel remote work,
classify every error as a committed write, persist credentials or form inputs,
recover an intent after explicit dismissal/reload, or change API authorization,
licensing, native catalog, scientific admission, or idempotency semantics. Changed
request content remains a different logical intent under the existing contract.

## Verification

`npm run typecheck`, all **545** web unit tests (including three dismissal-policy
checks), and `npm run build` passed locally. Native generation remains unchanged;
the PWA build retains the existing size gate.

Three synthetic browser cases are included in `test:auth-ui`: all four editors'
dirty Cancel/Escape/continue/abandon flows; pristine unsubmitted dismissal; and an
unchanged pending source update followed by lost responses and an exact key/body
retry. These are browser interaction fixtures, not real data-source admission or
license operations. This executor cannot create Chromium's required sockets, so
hosted execution of these cases and the separate existing real Rust/PostgreSQL
browser acceptance is required before merge. No browser pass is claimed locally.

Independent native review and all applicable final-head CI remain required.
Target dev only. This batch does not change the operator plugins or their current
source/PIT limitations.
