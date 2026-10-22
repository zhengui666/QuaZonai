# Traceable literature context during research

**Source:** User request for automatic relevant-paper discovery during research, developed from current remote `dev` for reviewed delivery back to remote `dev`.  
**Task ID:** `research-literature`.  
**Base:** `a233b966` on `codex/research-literature-dev`.  
**Issue / PR:** Not created.

## Scope

Use the current TypeScript and official Pi Durable architecture. The built-in `qz.research` tools search Crossref, arXiv and Semantic Scholar for public paper metadata and available abstracts, preserve the source records and bind the Agent's evidence notes to those records. Store literature facts in a separate `qz.literature` document in the existing official Durable database and expose them in research detail and its Web literature tab.

Public queries exclude private inputs and credentials. This work adds no paper-provider credentials, full-text reader, paywall bypass, second task database or change to scientific qualification. Citations support hypothesis development; they do not prove out-of-sample performance or profitability.

## Acceptance

- Research tools search public scholarly sources and persist the original task identity, query, source outcomes, paper metadata and available abstracts.
- Notes identify the saved search and paper, a supporting/contrary/background claim, applicability limits and any excerpt from retrieved material. References absent from the saved search and excerpts absent from its retrieved abstract are rejected.
- Committed search results survive close/reopen and replay. Repeated reads and normalized queries reuse saved facts. A new task with explicit `retryFailed:true` may retry a latest partial/unavailable result, preserving its old failure; original task replays retain their original receipt. Literature operations do not consume scientific attempts.
- No search, empty successful results, partial failure and unavailable providers remain distinguishable. Missing abstracts and metadata stay absent; no full-text-read claim is inferred.
- Service and Web expose saved literature, source links, notes, limitations and failures while retaining original scientific input, trial and evaluation behavior.
- Applicable final-Head CI passes, actionable findings are resolved and GitHub Codex gives an explicit clean review before merging into remote `dev`.
- Real Agent evaluation is reported separately from official Faux/provider fixtures and storage/protocol checks.

## Implementation and verification status

Implementation and local integration are complete on the remote-`dev` baseline above. At authoring, final-Head CI, GitHub Codex review and merge remain pending; no release or deployment is claimed. Local verification:

- `make check-ts` passed: service 115 tests, research 141 tests and CLI seven tests, with all three type checks passing and no skipped tests.
- `make check-web` passed, including 23 tests and the production build. `make check-deploy` passed; one release-install check was skipped because its real release prerequisite was not supplied. These checks do not establish an installed service.
- Independent review found no actionable findings and independently ran all 13 new transport/Durable tests successfully. HTTP/Durable restart and two UI rendering checks also passed; these establish deterministic storage and presentation behavior.
- The six changed Markdown files passed the scoped offline link check: 44 links, zero errors. `git diff --check` passed.
- Full-repository `make check-links` ran and failed on 59 existing stale references in 18 unchanged historical/other documents; none is in this change's six documents. Those unrelated records are preserved.

The final PR carries CI/review receipts; the project board records the observed merge result.

The configured official model catalog supports `gpt-6-luna` with `max` reasoning, but this environment currently has no authenticated model account and its network policy does not allow the required model/paper API destinations. The real Agent evaluation is blocked on those prerequisites. Faux-provider runs establish protocol, persistence and recovery behavior only, not actual Agent research quality or successful live literature retrieval.

## Delivery

Completion requires the reviewed final change to be merged into remote `dev` under the [review rules](../../review.md). Release, installed-service acceptance and real-account evaluation remain separate states and require their own receipts.
