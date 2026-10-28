# Clarify feature-v2 masks in research briefs

## Task

Task ID: `feature-v2-mask-guidance`. The owner requested a minimal authoritative ABI clarification after four counted Hyperliquid models rejected valid inputs. Deliver through the existing PR/CI/review flow into `dev`; preserve failed attempts and the shared budget.

## Requirements and design

`research_brief` already exposes the feature-v2 ABI for tick and catalog-ensemble research, but only says missing masks are not observed zero. State the actual setter argument order and codes: observed finite=0, NotYetAvailable=1, Expired=2, SourceMissing=3. Explain that nonfinite predictions fail the counted trial and are not missing-output markers. Tick briefs also disclose native EMA/previous-cohort warmup skipping. Reuse the existing shared projection for both feature-v2 paths. `catalog_input` remains a source-selection descriptor; it exposes no model ABI.

No native contract, computation, trial admission, old facts, model selection or held-out behavior changes. The four operational WAT corrections retain their formulas and are not part of this source patch. Failed attempts remain counted.

## Verification

- Read actual persisted model tool calls and native setter: generated WAT returned NaN when mask was zero; native assigns zero to observed finite values. Native tick prediction already skips incomplete warmup.
- `node --experimental-strip-types --test apps/research/test/feedback.test.ts apps/research/test/tick-input.test.ts`: 13 passed.
- `npm --prefix apps/research run typecheck`: passed.
- `git diff --check`: passed.
- No local Cargo, source-data replay or new factor trial run for this change. Final-head CI and independent review are pending.
