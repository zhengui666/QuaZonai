# Rebalance publication rollback

## Evidence and scope

[PR 177](https://github.com/zhengui666/QuaZonai/pull/177)'s
[full regression](https://github.com/zhengui666/QuaZonai/actions/runs/37633722771/job/112847872105)
failed the original automatic-rebalance case at its final concurrent Release
assertion: both retries returned no Release after an intentionally failed
publication. The original Build, Study and formal PASS had already succeeded.

SQLx transaction Drop queues rollback for later connection cleanup. Until that
cleanup runs, the failed publication can retain the project row lock and both
immediate `FOR UPDATE SKIP LOCKED` retries can skip it.

## Change and acceptance

- Await rollback before returning a Release publication error; propagate rollback
  failure as a database error rather than suppressing it
- Deterministically withhold the failed connection's pool-return cleanup while
  executing the original concurrent retries, then release the gate, including
  during panic unwinding
- Preserve all original cardinality, identity, immutable-package, receipt and
  approval assertions, as well as qualification and expiry rules
- Add the same original Store case to the existing exact release-regression
  runner, raising its count to eleven and the selected gate total from 33 to 34,
  without removing any selector or changing full regression
- Run the original Store `experiment_compilations` and Server
  `portfolio_study_http` cases against disposable PostgreSQL/PGMQ; run applicable
  final-revision routine and full Rust regression before delivery

Local validation and exact final-revision CI results must be recorded separately;
source inspection does not establish a PostgreSQL regression pass.
