# Research workbench redesign

## Goal

Replace the default administrative project-table landing page with a focused research workbench. The owner explicitly removed the mandatory Ant Design restriction; official components remain useful for existing authoring and recovery flows. Do not rewrite the Rust backend or fabricate research data.

## Design and scope

- A compact, differentiated workspace shell with stable section navigation and theme controls.
- A concise research path, real project cards, current Brief state, last update, and direct continue/edit actions.
- Page-local search is explicitly labelled; the server cursor remains authoritative and pagination is retained.
- An actionable zero-project state and links to existing Alpha, portfolio and run surfaces.
- Desktop and mobile, light and dark modes. Existing mutations, receipts, dirty guards, PWA and lazy-loading boundaries stay intact.

## Verification

Use the [web checks](../../project.md#commands). Native browser tests retain their lost-ACK, exact replay, restart, viewport, unsaved-edit and theme assertions, adapted from table rows to project articles. Real Rust/PostgreSQL screenshots must be labelled with their exact revision and isolated acceptance data. Transport fixtures are not end-to-end backend evidence.

Publication and independent review are coordinated by the dev owner; final-head CI and native screenshot evidence are required before merge.
