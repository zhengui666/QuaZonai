# Button state contrast

## Evidence and scope

The native-data browser run for PR 149 at
`de8d3b13efb1855ed10b33f7ea5ad105cbf79e9f` passed the mobile project flow but
failed the unchanged Settings accessibility sweep in light mode at 1440px.
The enabled Login ChatGPT button had foreground `#adb4c3` on `#b6c4e1`
(1.18:1), and enabled Refresh had `#8a8a8a` on `#f8f8f8` (3.25:1).
Neither recorded button had a disabled attribute or loading class. Ordinary
Web acceptance passed on the same Head, so the failure is timing-sensitive.

Pinned official Ant Design 6.5.2 generates `transition: all` using
`motionDurationMid` on the Button root in `antd/es/button/style/index.js`.
Its `variant.js` disabled rule changes both text and background colors.
Native CSS extraction confirms the default duration is
0.2s, while loading-icon width/opacity/margin use `motionDurationSlow` (0.3s).
The reported colors are consistent with an intermediate disabled-to-enabled
frame; enabled attributes do not imply the color fade has finished.

[ChatgptAuth](../../../apps/web/src/chatgpt-auth.tsx) loads account operations
after the profile appears and reports busy state through an effect.
[ProfileDetails](../../../apps/web/src/codex.tsx) can then start a probe, and
terminal account observations can invalidate queries again. These paths permit
new button transitions after the audit's finite animation snapshot. The artifact
does not identify the exact request/effect ordering; no local browser replay is
claimed. Waiting for a later ready state would hide this enabled-state contrast
gap from the audit rather than correct what a user can see.

## Correction and tradeoff

Use the public `ConfigProvider.theme.components.Button.motionDurationMid`
override with `0s`. Native CSS extraction verifies this emits a component-scoped
`.quazonai.ant-btn` duration override while the global middle duration and slow
loading-icon duration stay unchanged. Button-root transitions, including surface
colors and shadows, become immediate; that fade is the intentional visual tradeoff. Loading
glyph motion, busy/disabled semantics, focus, other components and request/
receipt behavior remain unchanged. No upstream implementation is modified.
The four original mobile change files are preserved byte-for-byte.

## Regression and verification

Add two synthetic browser regressions, one per theme, using the real application
and official primary/default buttons. Offline/online events drive their actual
disabled state. After observing the first enabled mutation, force style
resolution and freeze any native color transitions at 30% before running the
real axe color-contrast rule. This makes a failing intermediate frame observable
without a fixed sleep, retry, relaxed threshold or waiting out the transition.
Both buttons must actually become enabled; keyboard focus is checked too. The
existing full native-page axe sweep is unchanged.

Run the complete lightweight frontend checks and retain exact-Head hosted
synthetic and native browser acceptance as required gates. Local browser startup
is unavailable under the executor's established socket restriction.

## Results

Local `make check-web` passed on this base: 350 generated response-contract
files verified without drift, 42 validator-adapter tests, typecheck, all 579
Web unit tests, production build and the modular startup/PWA-precache assertion.
The final theme assertion added to the browser regression received another
passing typecheck. `git diff --check` and both new local document links passed.
The full Markdown checker is unavailable because lychee is not installed.

An additional native CSS extraction check asserted the scoped zero-duration
override, unchanged 0.2s global middle duration, unchanged 0.3s global slow
duration, and identical loading-icon transition rules before/after the override.
It used the installed official packages; no dependency or lockfile changed.

The new synthetic browser regressions and final-Head native acceptance have not
run locally. Hosted execution and independent review are still required before
merge; the earlier hosted failure is not claimed to be fixed by a
browser run yet.
