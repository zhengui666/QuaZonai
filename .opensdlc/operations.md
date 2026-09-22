# Engineering operations

Production procedures live in [OPERATIONS](../OPERATIONS.md) and the [hosting guide](../docs/user-guide.md).

<a id="controls"></a>
## Action boundaries
Modify only task-owned source. Preserve user data, credentials, migrations, licenses and unrelated changes. Normal CI has no production secrets; deployment and account operations are separate actions.

<a id="delivery"></a>
## Delivery and recovery
Use a new branch and PR, actual [checks](../CONTRIBUTING.md#verify-the-change), and the [review policy](review.md). Record current-Head evidence once in the task/PR. Revert a source change with Git; never erase data to simulate rollback.

<a id="observe"></a>
## Observe and respond
Read failed Actions logs, identify the narrow failing contract, fix source and rerun affected checks. Superseded branch runs may be cancelled; the newest Head still requires its own results. No extra polling service or compliance inventory is required.

<a id="metrics"></a>
## Measures
Report actual failures, command outcomes and measured timings with their workload/profile. Do not infer production readiness or whole-system speedups from a smoke test or microbenchmark.
