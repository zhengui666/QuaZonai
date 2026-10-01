# Mobile project summary

## Requirement and evidence

Baseline: `00e80c7412f1725de874b6b6327e7011bc479924` on `dev`.
The native browser's 390px project screenshot shows only the project name:
the fixed 760px table moves state, current Brief, updated time and Edit outside
the visible viewport. The desktop light/dark screenshots expose those fields.
The [Web architecture](../../architecture.md#web) requires the same operations
on narrow screens.

## Design

Keep one official Ant Design Table and use its Grid breakpoint to present a
single summary column below `md` (768px). Each summary exposes the existing
project-name action, StateTag, Brief value or `尚未选择`, formatted updated time
and Edit button. Official Descriptions supplies the labelled metadata and theme
styles. Long names and Brief identifiers wrap within the column. At `md` and
above, keep the existing five columns and horizontal table scrolling.
The initial render is compact until Grid's breakpoint subscription reports the
viewport; this transient layout choice does not change selection or editor state.
The compact updated value uses semantic `time` markup with the API timestamp.

Only table presentation depends on the breakpoint. Preserve the query, cursor
history, refresh, empty/error/offline handling, callbacks and the single
ProjectEditor outside the table. Resizing or changing theme while editing must
retain form values, dirty confirmation and command intent. No API, receipt,
research qualification, dependency or navigation contract changes.

## Verification plan

Extend [native acceptance](../../../apps/web/tests/native-console.spec.ts) to
assert that the entire mobile name, state, Brief, timestamp and Edit action are
inside the viewport before a click can auto-scroll them into view. Check the
table has no horizontal overflow at 390px in both themes. Exercise editing,
resize across the breakpoint, cross-tab theme changes, continued editing and
discard, then confirm the real project remains unchanged. Preserve the existing
desktop/tablet, screenshots, lost-ACK and restart checks.

Run the frontend generation/no-drift, validator-adapter, type, unit, production
build and lazy-precache checks. The executor's known socket restriction blocks
local browser startup; native browser acceptance is required on final-Head
hosted CI, alongside independent native review, before merging to `dev`.

## Results

Integrated onto `20c763fa6a6db327eed705b5f6d69b3e908a9316`; all four scoped
files remained byte-identical when moving to the new base. Final semantic time
markup and its test were then added on that base.

Local `make check-web` passed: 350 generated response-contract files verified
without drift, 41 validator-adapter tests, typecheck, all 568 Web unit tests,
production build and the modular startup/PWA-precache assertion. After the last
markup/test edit, typecheck, all 568 unit tests, production build and the modular
startup/PWA-precache assertion passed again. `git diff --check` passed.

`make check-links` is blocked because lychee is not installed. The two new
task-note links and their explicit anchor were checked locally. Native browser
tests have not run in this executor, and no post-change screenshot is claimed.
Hosted final-Head browser acceptance and independent review remain required;
these local checks make no Agent-quality or browser-history claim.
