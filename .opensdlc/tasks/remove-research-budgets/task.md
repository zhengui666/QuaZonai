# Remove research execution budgets

Issue: [#276](https://github.com/zhengui666/QuaZonai/issues/276). PR: [#277](https://github.com/zhengui666/QuaZonai/pull/277).

## Goal

Remove execution quotas from remote `dev`: trial counts, model turns/tokens, scientific calls, native execution time, fuel, data row quotas and worker resource ceilings must not stop or reject research. Keep actual usage, immutable inputs, original trial/request identities, independent evaluation, owner stop and recovery. Account funds, liquidity and scientific method definitions retain their original meaning.

## Acceptance

- Research and PM submissions do not require a trial budget; the Web and CLI no longer request one.
- Old persisted quota fields and old client inputs remain readable but cannot impose execution limits.
- Research, review and PM continue past historical quotas while recording every actual attempt and receipt.
- Native worker execution does not impose default memory/output quotas or user fuel/time quotas.
- Appropriate TypeScript, browser and native boundary checks pass; independent review covers the final Head before merging into remote `dev`.

## Delivery

Current TypeScript research/service, Web/CLI, remote-worker and native scientific entrypoints no longer enforce execution quotas. Legacy request fields and ledger records remain readable; registered data and original request identities are retained. Low-level metering APIs and the retired managed-job protocol are outside the current product execution path.

At candidate creation: research 263/263, Service 391 passed (28 native-fixture-dependent checks skipped), Python source tools 235/235, CLI 15/15, Web unit/type/build and worker 13/13 passed. Additional regression checks cover legacy zero/exhausted quotas, 65 sources, 129 clusters, token dispatch, recovery and original receipts. Full browser and native checks continue against the PR candidate.

`make check-deploy` passes with the installed complete Node distribution. Changed-document links pass. Repository-wide `make check-links` reports the same 59 pre-existing failures on the original `dev` base and this candidate. No link failures were added.

Final-Head hosted CI and explicit clean Codex review are required before merging into remote `dev`. The linked PR records final verification, review and merge evidence; no production deployment or account operation is included.
