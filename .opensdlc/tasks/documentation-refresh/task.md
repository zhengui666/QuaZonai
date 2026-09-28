# Documentation and prebuilt deployment

**Source:** [Issue #120](https://github.com/zhengui666/QuaZonai/issues/120).  
**Task ID:** `documentation-refresh`.

<a id="spec"></a>
## Specification

The Issue owns the deployment field map and acceptance scope. The correction starts at `0b95f196b955674e534e5eabcdfbf80ba07ed3f2` on `fix/ghcr-only-deployment`; PR #121's earlier checks do not cover it.

<a id="verification"></a>
## Verification

The corrective PR records native tests and exact-Head review. The existing container suite covers installation, update, retry, data preservation and native Codex; its extension exercises the packaged Runtime and blocks host build commands. Version publishing additionally runs a no-checkout GHCR installation on a fresh runner. A configured job is not an executed result.

<a id="delivery"></a>
## Delivery

Endpoint: PR, all applicable final-Head CI and clean read-only Codex review, then merge. Image publication requires its actual workflow receipt; no production installation or account operation is part of this task.
