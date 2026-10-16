# Durable native Paper claim admission

## Scope and baseline

The original authenticated Paper control kept the accepted claim only in process memory. Starting another process cleared that memory. Polymarket also reserved a new evidence directory, but changing that directory did not protect the claim identity. This task adds durable admission to the existing control service and explicit terminal recording at its two existing owner entrypoints. It does not add a Hyperliquid Paper host, expand capabilities, remove existing Binance code, alter account semantics, fork Nautilus, or send venue orders.

## Required configuration migration

Both `job paper serve` configuration (`PaperConfig`) and Polymarket service configuration now require `claim_state_directory`. Set it to one absolute, canonical, private directory on the existing runtime's persistent local volume. Its existing parent must be present. Reuse exactly this state root for all service restarts and evidence-directory changes. There is deliberately no default derived from the current working directory, credentials, output directory, or temporary directory. Missing configuration fails closed. A new Polymarket output directory cannot contain the state root.

Unix file/directory locking, same-filesystem no-replace rename and directory fsync must be supported. Unavailable/unsuitable storage rejects admission. The state root and each created scope directory use mode 0700; records and lock files are private. Do not replace an active state volume, change the root to evade a claim, or remove claim/lock files. Different physical roots/runtimes do not coordinate; operators must preserve and back up this runtime state with the runtime. Previously executed claims from before this change are not automatically imported, so rollout must not replay old claims as new work.

## Scope, transitions and crash boundaries

The durable key is `(state root, adapter, project_id, downstream_id, handoff_id)`. The entire original typed claim must match; changes to its package or external claim ID conflict. Adapter is the existing control-owner profile, not an operator-supplied exchange class. Capability versions and session output directories do not partition the key. Distinct legitimate claims use separate OS locks and are not permanently blocked by another claim's incomplete state. Each process still admits only one new session.

1. Sync the parent of the stable root and every scope directory even when the directory already exists; a previous failed sync or concurrent creator does not establish durability. Acquire the per-claim `owner.lock` with nonblocking `flock`; keep its file descriptor alive while the native owner may run. Never delete/recreate the lock inode.
2. Serialize the immutable original claim to a private staged file, sync it, atomically publish `claim.json` without replacement, and sync the containing directory.
3. Only after all publication steps succeed may the request be queued to the native owner.
4. The original owner must actually finish or join before `retain_terminal_observation` stores its observed terminal `PaperStatus`. Failed and cancelled observations remain failed/cancelled; neither a successful constructor nor a stop request completes the claim.
5. Every terminal observation has `schema_version: 1` and must deserialize as the complete `PaperStatus`, including explicit nullable fields, correct field types, supported state and valid timestamp. Unknown fields/versions and incomplete records fail closed, even if identity fields match. Validation does not reserialize or change response bytes. Terminal observation publication also uses a staged file, file sync, no-replace rename, and directory sync. A completed replay returns those same response bytes with `X-Paper-Claim-Replayed: true`, without queuing execution or consuming the new service's available session. Its `/status` remains the current service's own status.

A crash before reservation publication cannot have queued execution; abandoned staging files alone do not reserve a claim. A crash after durable reservation but before queueing is deliberately indistinguishable from an execution whose result was lost. A crash during execution, or after native termination but before terminal publication, leaves the original reserved and returns `paper_claim_recovery_required`. A locked claim returns `paper_claim_in_progress`. Malformed/partial claim or terminal records fail closed. The OS releases a crashed process's lock; release never authorizes another execution. A completed receipt is read even after original delivery validity expires. No failed write is silently treated as successful durable completion.

## Recovery semantics and limits

The returned receipt is the original **lifecycle observation**, not a QZ approval, fill receipt, source-evidence certificate, or restored account. `FRESH_ACCOUNT_AND_SESSION_NO_RESTORE` remains unchanged. Original evidence/report files remain in their original locations; this journal does not fabricate or reconstruct them.

- A live lock means the existing owner may still be executing; inspect its original control/status and evidence before deciding anything.
- An incomplete or damaged record requires reconciling the original owner and original retained evidence. Do not blindly retry execution, delete the reservation, change output/state roots to get around it, or fabricate a terminal observation.
- A normally terminated failed/cancelled owner records that genuine terminal status and subsequent duplicate calls receive it unchanged. A distinct newly authorized claim can be admitted separately.
- This minimal candidate intentionally does not add a manual state-reset API, automatic account restoration, or an evidence-import recovery command. An interrupted claim remains blocked until a separately reviewed recovery path establishes its actual original outcome. This trades a possible false-positive block for prevention of duplicate execution.

## Verification

Targeted Rust tests cover actual subprocess lock contention and process exit without destructors; simultaneous openers; incomplete restart; exact completed and failed receipt replay; different claim/adapter/project/downstream isolation; changed originals; malformed and partial records; abandoned staging files; terminal publication failure; invalid/nonterminal completion; symlink rejection; changing evidence directories; current control-server HTTP replay and owner completion; and no duplicate queueing across services.

Run the repository's final-head required checks and independent review before integration. Targeted synthetic control/storage tests do not prove a native-market run, research admission, account continuity, or production deployment.
