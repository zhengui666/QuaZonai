# Recovery

| Observation | Action |
| --- | --- |
| Login missing/revoked, 401/403 | Stop writes; user repairs the original login in their private terminal. Never read credentials or broaden authority. |
| Invalid input, unknown command/schema or unsupported adapter | Inspect exact help/source schema and report the missing capability. No alternate endpoint or fake success. |
| Disconnect, timeout or `write_outcome_unknown_retain_original_request` | The write may have committed. Keep the original body and `requestId`, read known IDs, and replay only that unchanged original start when needed. Never generate another key. |
| `request_id_conflict` / HTTP 409 | Read original state and compare with intent. Do not substitute a changed body/revision or overwrite history. |
| Trial budget or no-progress continuation limit | Inspect the recorded stop reason and complete trials. Do not reset counters, create free attempts or silently extend the study. |
| Quota/billing/auth failure | Report the final native failure. No automatic model/provider fallback or paid inference probe. |
| `heldout_already_exposed` / missing independent evidence | Preserve the original exposure/selection; renaming, copying or forking cannot make observed data unseen. |
| Watch timeout/detachment or missing native submission projection | Observation ended, not admitted work. Read the same ID; disclose unknown settlement rather than infer success from IDLE/empty tasks. |
| Unknown stop/logout/downstream outcome | Reconcile the original resource/device. A receipt, closed connection or local exit does not prove remote cancellation/revocation. |

Keep only approved nonsecret original requests, resource IDs and receipts. Never edit native SQLite, clear budgets/trials/exposures, rediscover credential files or create a replacement task loop.

Return requested operation → original IDs → actual observed state → evidence/limitations → the next required action. Distinguish running, failed, stopped, unknown, not attempted and computed-but-unqualified.
