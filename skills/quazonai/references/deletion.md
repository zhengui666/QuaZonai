# Delete requested objects

Delete only the specific objects within the user’s requested scope. Inspect their current identities, state and dependencies first; use exact IDs, not a title match or a guessed list position. Existing explicit authorization applies without another confirmation. Deletion is an owner-visible logical removal, not database purging.

```sh
quazonai pm get MANDATE_ID
quazonai pm delete MANDATE_ID
quazonai research get CONVERSATION_ID
quazonai research delete CONVERSATION_ID
quazonai factor get FACTOR_ID
quazonai factor delete FACTOR_ID
quazonai portfolio get PORTFOLIO_ID
quazonai portfolio delete PORTFOLIO_ID
```

`pm` addresses an investment decision mandate. Research IDs are original numeric conversation IDs; factor IDs are original `r<research>.t<trial>.m<member>` references; mandate and portfolio IDs are opaque. Each delete sends one `DELETE` to that original resource with no request file or replacement ID. Success returns `{kind,id,deletedAt}`. Read the corresponding list afterward to verify removal. Normal detail reads then return 404; the original evidence, trials, exposure, requests and account/target receipts remain stored for lineage and recovery. Deletion is not a way to reset scientific eligibility or rerun exposed data.

Running work or an unresolved dependency is rejected with HTTP 409. Report its actual error code and inspect the original resource; do not bypass it by editing storage, clearing references, deleting a parent or inventing a different endpoint. Resolve a dependency only within the user’s authorized scope. Research stop, PM retirement, target reconciliation and account position handling remain separate operations; a delete request does not authorize them or prove remote cancellation, flat positions or completed retirement. Existing downstream observations and their account/receipt responsibilities must remain available.

After a lost response or `write_outcome_unknown_retain_original_request`, do not claim deletion failed or succeeded. Preserve the original kind and ID, inspect its list/current read, and explicitly replay the same delete when needed to recover its idempotent original receipt. The client never retries a write automatically. Repeated deletion of the same existing tombstone returns the original receipt; a 404 without a receipt can also mean the ID never existed, so do not present that alone as proof of deletion.

Report original IDs, returned receipts, verified list removal and any blocked objects separately. A request to add deletion support to the product does not authorize deleting objects in a user’s running instance.
