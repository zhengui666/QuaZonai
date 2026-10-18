# Reduce backend research API response time

<a id="task"></a>
## Task

**Task ID:** backend-rt

**Source:** Owner request: “Quazonai 的后端接口 rt 特别长，调研并优化一下”; then explicitly requested a new branch from remote `dev`, a PR and merge into remote `dev`.

**Endpoint:** Investigate, implement and verify a focused optimization; deliver through a PR to remote `dev` under the repository's final-Head CI/review rules. No installed-service restart, deployment or research model inference.

<a id="intent"></a>
## Intent

Reduce slow backend reads without changing scientific evidence, identities, recovery or authentication. Preserve unrelated changes in the original checkout; implementation uses `.pi/worktrees/backend-rt`, branch `perf/backend-rt`, based on remote `dev` at `cd64dad9`.

<a id="spec"></a>
## Requirements and design

- Research list must produce summaries directly, not construct full trial metrics and historical task details that it immediately discards.
- Usage must inspect native live tasks without also scanning 100 historical task receipts.
- Detail retains its latest 100 task views, newest first. Cache only public metadata in memory, hydrate in pages of ten through the official scan API and advance it from official committed task events. No TTL, copied execution authority, database schema changes or modified upstream code.
- Original/current submissions, captured dataset identity, errors, missing values and full detail metrics retain their meaning. GETs do not start work.
- Limitation: upstream 1.1.0 (and inspected 1.3.0) has no metadata-only task scan. After process restart, the first detail read for each conversation still loads historical task records once. Do not conceal this with a fabricated empty history or manipulate Durable's private SQLite schema.

<a id="plan"></a>
## Implementation plan

1. Add regression checks in `apps/service/test/projections.test.ts` using existing test support and official storage/task APIs: summaries agree with details, list/usage do not scan terminal history, cached details update on commits and keep newest-100 order, concurrent hydration shares one scan.
2. Split shared research summary projection in `apps/service/src/projections.ts`; use native `inspect()` for list liveness. Keep full detail projection separate.
3. Add task metadata read caching through public `subscribeCommits` and `Tx.scanTasks`; bind its lifecycle to the service and use it for all detail/stop response paths. Change `/usage` to use native inspection directly.
4. Compare the old/new routes on the same controlled workload; run `make check-ts`, relevant Web/deployment checks and final-Head CI/review. Record observed results, not assumed passes.

<a id="verification"></a>
## Investigation

Installed container reports source `cd64dad9`, matching the worktree baseline. Read-only installed CLI requests (three runs, including CLI startup):

| Request | Elapsed ms |
| --- | --- |
| identity | 333, 342, 334 |
| settings get | 348, 363, 329 |
| usage | 10027, 8096, 8349 |
| research list (11 rows) | 12532, 12274, 15256 |
| research get (193 trials, 100 tasks, 1.73 MB JSON) | 19109 (one run) |
| portfolio list | 237, 248, 239 |
| observation list | 215, 221, 221 |
| datasets | 209, 219, 215 |

`/health/live` via localhost HTTP: 1.28 ms. Production database inspected with SQLite `mode=ro`, without opening another Harness: 743 terminal tasks, about 9.684 GB task JSON; 236 `qz.strategy-portfolio.evaluate` receipts account for about 9.683 GB, max 147,865,014 bytes. Latest 101 task records total 1,449,311,224 bytes. Native results are in terminal outcomes, not necessary for status projections. `qz.study` history totals about 13.2 MB, and native usage documents are small.

Code confirms `/research` sequentially calls `researchDetail` per row, scans 100 full task records each, generates every trial's metrics, then drops detail fields. `/usage` calls `research.tasks()`, which performs native inspection **and** an unused 100-record historical scan.

No production data was modified, no service restarted, no raw evidence/credentials recorded here.

### Implemented and checked

The two regression tests failed on the original implementation: list/usage performed historical scans, and concurrent details performed two scans rather than sharing hydration. After the fix, tests cover summary-field equality, no historical scans on list/usage, concurrent hydration, active/failed task updates, newest-100 retention and ten-page cold loading. Existing public-error and dataset-identity checks remain.

`make check-ts check-web check-deploy` passed on the implemented tree: 73 Service, 58 Research, 4 CLI, 5 Web tests, Web production build, 227 source-helper tests, 15 deployment/native packaging checks plus 5 Python packaging/release checks. One real-release distribution installation check was skipped because no release distribution was supplied; it is not reported as executed. No native scientific logic changed; applicable exact-Head native/core and browser CI remain required on the PR.

### Same-snapshot HTTP comparison

A task-owned consistent SQLite backup was made through SQLite's online backup API with the production source opened read-only. Both original `cd64dad9` and changed service code ran against that disposable snapshot with only Faux models, no real providers, and zero model calls. Localhost HTTP timing includes reading the complete response body, without CLI startup. Both runs used Node v26.7.0 and `--max-old-space-size=8192`, matching the installed process's 8 GiB old-space allowance (installed Node is v22.19.0). Local checks overlapped part of the baseline; these are diagnostic samples, not isolated-load percentile claims.

| Route | Before, ms (3 requests) | After, ms (3 requests) |
| --- | --- | --- |
| `/api/v3/usage` | 13673, 14269, 17920 | 36.33, 4.52, 3.15 |
| `/api/v3/research` | 21954, 19563, 19468 | 318.19, 7.39, 5.77 |
| `/api/v3/research/:id` | 11161, 10605, 8761 | 8492.03 cold, 37.55, 18.86 |

All requests succeeded; both versions returned 11 studies, 193 trials and 100 detail task views. Full response byte lengths were unchanged for all three routes (73, 54089 and 1732288 bytes). The first baseline attempt under Node v26's default ~2 GiB heap ran out of memory; that failed run is not a timing result. Small-page cold hydration reduces the allocation batch; cold detail still does real evidence I/O once after restart. No persistent cache, upstream source patch or schema migration was added.

<a id="review"></a>
## Review

Author review covered all `researchDetail` callers, native inspection, original/current submission projection, cache commit ordering and service lifecycle. Independent native read-only review and explicit GitHub Codex read-only review must identify the final PR Head; their actual results and CI belong to the GitHub PR, not a prefilled local acceptance claim.

<a id="delivery"></a>
## Delivery

Branch `perf/backend-rt` starts at remote `dev` (`cd64dad9`). [PR #213](https://github.com/zhengui666/QuaZonai/pull/213) owns the final Head, actual CI/review and merge state. Merge only after final-Head checks and required reviews pass. No deployment, production restart, installation change or scientific-result rewrite is included. The original main checkout's unrelated changes remain untouched.
