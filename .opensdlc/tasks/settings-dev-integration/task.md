# Preserve autosave integration and selective validators

The external main-to-dev merge at `71a0d49f09d8e554474ad420731b53aabcb1c537`
introduced five failing browser cases in the settings suite. The same five fail
on the unchanged integration and on PR 148's first integration head. The first
three never reach the portfolio project selector; the final two encounter a
confirmation modal when leaving an already retained uncertain source request.

Restore the portfolio's selective base-currency validator entrypoint. The
aggregate CommonJS compatibility facade is intentionally outside the browser's
selective dependency list. A TypeScript AST check now prevents application runtime
imports from silently reintroducing that facade, without restricting test/Node
compatibility or type-only imports.

Allow editor detachment only when its caller explicitly retains the exact
request outside the editor. Pending writes still cannot close. Unsubmitted dirty
forms and local failed attempts still require confirmation. The four data command
dialogs use their existing persistent SettingsCommand unknown state; request
closures, bodies, revisions, keys and receipts are unchanged. This does not start
a retry or infer completion/cancellation.

Keep all five browser cases and their exact-body/revision/receipt assertions.
Add explicit route-heading and dialog-dismissal assertions so future failures
identify the broken boundary before unrelated clicks. Existing dirty-input,
Escape, pending-write and autosave recovery coverage remains mandatory.

The cloud executor cannot run local browser/socket acceptance. Local source,
unit, build and generated checks and independent review precede publication;
the final head must pass the complete hosted Web and native gates. No prior-head
result or local inspection establishes a current browser pass.
