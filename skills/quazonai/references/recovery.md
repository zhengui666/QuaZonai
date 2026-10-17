# Recovery

| Observation | Action |
| --- | --- |
| Login missing/revoked, 401/403 | Stop writes; user repairs the original login in their private terminal. Never read credentials or broaden authority. |
| Invalid input, unknown command/schema or unsupported adapter | Inspect exact help/source schema and report the missing capability. No alternate endpoint or fake success. |
| Disconnect, timeout or `write_outcome_unknown_retain_original_request` | The write may have committed. Keep the original body and `requestId`, read known IDs, and replay only that unchanged original start when needed. Never generate another key. |
| `request_id_conflict` / HTTP 409 | Read original state and compare with intent. Do not substitute a changed body/revision or overwrite history. |
| Original trial budget exhausted | Inspect the recorded stop reason and complete trials. Do not reset counters, create free attempts or silently extend the study. Response/yield counts are not a scientific stop rule; the running native host supplies successive generations for unfinished studies. |
| Quota/billing/auth failure | Report the final native failure. No automatic model/provider fallback or paid inference probe. |
| Original native submission `unanswered/model_error` | Read `research diagnostics ID`. Only with owner authorization, after reconciling quota/auth and the failure, use `research resume ID --submission-id ORIGINAL_FAILED_ID`. It continues the same conversation under the unchanged ledger/budget, with a new receipt linked to the failed submission; old failures are never rewritten. If uncertain, replay only those same IDs, not a replacement study/key. |
| Latest native input `done`, unselected Study `STOPPED` with `trialId:null` and `result:null` | Only with owner continuation authorization and remaining original budget, use `research resume ID --submission-id PREVIOUS_DONE_ID`. The new input stays in the same study and links its predecessor; the entire old stop remains in owner-visible `previousStops`. Selected/assessed finals and aborted/active inputs cannot continue. A later explicit owner stop wins over an interrupted admission; do not retry with another ID. |
| `heldout_already_exposed` / missing independent evidence | Preserve the original exposure/selection; renaming, copying or forking cannot make observed data unseen. |
| Watch timeout/detachment or missing native submission projection | Observation ended, not admitted work. Read the same ID; disclose unknown settlement rather than infer success from IDLE/empty tasks. |
| Unknown stop/logout/downstream outcome | Reconcile the original resource/device. A receipt, closed connection or local exit does not prove remote cancellation/revocation. |

Keep only approved nonsecret original requests, resource IDs and receipts. Never edit native SQLite, clear budgets/trials/exposures, rediscover credential files or create a replacement task loop.

Return requested operation → original IDs → actual observed state → evidence/limitations → the next required action. Distinguish running, failed, stopped, unknown, not attempted and computed-but-unqualified.
