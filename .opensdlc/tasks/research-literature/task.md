# Traceable literature context during research

**Source:** User request for automatic relevant-paper discovery during research, developed from current remote `dev` for reviewed delivery back to remote `dev`.  
**Task ID:** `research-literature`.  
**Base:** `a233b966` on `codex/research-literature-dev`.  
**PR:** [#231](https://github.com/zhengui666/QuaZonai/pull/231).

## Scope

Use the current TypeScript and official Pi Durable architecture. The owner may specify 1–3 public search topics in the original `publicLiteratureQueries` input. The built-in `qz.research` tool accepts only a `queryIndex`; trusted code derives the query from that frozen list and searches Crossref, arXiv and Semantic Scholar for paper metadata and available abstracts. Preserve source records and bind the Agent's evidence notes to those records. Store literature facts in a separate `qz.literature` document in the existing official Durable database and expose them in research detail and its Web literature tab.

The model cannot compose arbitrary outbound queries from the goal or private research context. Topic validation and index-only selection do not establish that owner-entered text is nonsensitive. With no configured topics, return `public_queries_not_configured` and no source attempts; retain old receipts and allow scientific work to continue. This work adds no paper-provider credentials, full-text reader, paywall bypass, second task database or change to scientific qualification. Citations support hypothesis development; they do not prove out-of-sample performance or profitability.

## Acceptance

- Original raw and configured-dataset requests accept an optional list of 1–3 nonblank public topics, each at most 500 characters without control characters. The Web form saves one topic per line. The trusted tool derives queries only from valid indices into that list, without extracting text from the goal or specification.
- Research tools search public scholarly sources and persist the original task identity, selected query, source outcomes, paper metadata and available abstracts. Missing public-topic configuration makes no provider request and is not recorded as a provider failure.
- Notes identify the saved search and paper, a supporting/contrary/background claim, applicability limits and any excerpt from retrieved material. References absent from the saved search and excerpts absent from its retrieved abstract are rejected.
- Committed search results survive close/reopen and replay. Repeated reads and normalized queries reuse saved facts. A new task with explicit `retryFailed:true` may retry a latest partial/unavailable result, preserving its old failure; original task replays retain their original receipt. Literature operations do not consume scientific attempts.
- No search, empty successful results, partial failure and unavailable providers remain distinguishable. Missing abstracts and metadata stay absent; no full-text-read claim is inferred.
- arXiv requests use one in-flight connection per process and at least three seconds between requests; query text is escaped as a literal. Duplicate merging retains first-result ranking while preserving available abstract metadata.
- Service and Web expose saved literature, source links, notes, limitations and failures while retaining original scientific input, trial and evaluation behavior.
- Applicable final-Head CI passes, actionable findings are resolved and GitHub Codex gives an explicit clean review before merging into remote `dev`.
- Real Agent evaluation is reported separately from official Faux/provider fixtures and storage/protocol checks.

## Implementation and verification status

PR #231 is open. The four GitHub Codex findings are addressed in the revised implementation: owner-selected public query boundaries, process-wide arXiv pacing, literal query syntax and first-ranked rich metadata. At authoring, final-Head CI, renewed GitHub Codex review and merge remain pending. Verification of the revised code:

- `make check-ts` passed: service 117 tests, research 147 tests and CLI seven tests, with all three type checks passing and no skipped tests.
- `make check-web` passed, including 24 tests and the production build. The focused real Chromium test passed public-topic draft, validation, reload and uncertain-request replay. The complete 53-test browser suite is running separately.
- All 13 transport tests and six Durable tests passed, including privacy boundaries, pacing, ranking, literal queries, cancellation and recovery. Three HTTP/restart and three UI rendering tests passed.
- Earlier deployment helper checks passed with one real-release installation check skipped for its missing prerequisite. The revised workflow helper checks passed all four tests. Initial browser CI exhausted its 12-minute limit downloading dependencies from the Azure Ubuntu mirror; the revised workflow uses the official HTTPS mirror and retains all browser checks and the same limit.
- The revised six-document scope passed its offline link check: 45 links, zero errors. `git diff --check` passed.
- Full-repository `make check-links` ran and failed on 59 existing stale references in 18 unchanged historical/other documents; none is in this change's six documents. Those unrelated records are preserved.

These deterministic checks do not establish an installed service or real-model quality. The PR carries final CI/review receipts; the project board records the observed merge result. Release and deployment are separate outcomes.

The configured official model catalog supports `gpt-6-luna` with `max` reasoning, but this environment currently has no authenticated model account and its network policy does not allow the required model/paper API destinations. The real Agent evaluation is blocked on those prerequisites. Faux-provider runs establish protocol, persistence and recovery behavior only, not actual Agent research quality or successful live literature retrieval.

## Delivery

Completion requires the reviewed final change to be merged into remote `dev` under the [review rules](../../review.md). Release, installed-service acceptance and real-account evaluation remain separate states and require their own receipts.
