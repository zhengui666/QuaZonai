# Personal production

<a id="task"></a>
## Task

Continue `personal-production` against [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62)
and [DESIGN0.4](../../../DESIGN.md#acceptance-scope). The web assistant authors files;
GitHub Actions executes verification. Dedicated-account scope stays
**COMPLETED_BY_OWNER_WAIVER / NOT_RUN**, not a pass. Do not use or wait for CodexPro.

<a id="intent"></a>
## Intent

Verify the shipped API/Worker systemd user units with actual packaged services.
Existing unit syntax and `/usr/bin/true` scope checks do not prove QZ services run;
the prior real browser harness spawned the API directly. Reuse the existing
browser/database/Caddy flow instead of adding another supervisor or deployment platform.

<a id="spec"></a>
## Scope and authoritative sources

[DESIGN](../../../DESIGN.md) owns the product contract. The [acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance)
owns coverage and [user guide](../../../docs/user-guide.md#verify-changes-to-the-hosting-boundary)
owns the operator procedure. PR discussions retain per-turn execution, errors and review.

Base main is [PR #94](https://github.com/zhengui666/QuaZonai/pull/94) merge
`a6686641ade4fc2b2204605eb8aa2c7d4a292236`. Its accepted Head2b0b2c0 passed all five
workflows,688Store/Server,17OCI+2cold/ownership+1joint+6research+5science-thread,
17real hosted browser phases and explicit clean review5740783341. Seven findings
were resolved before merge. Real Validation/Reviewer/Sealed tests still grant no
qualification to FIXTURE data; this deployment change does not replace them.

Current branch: `codex/native-user-services-20260919`.
The [field-level deployment plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5740927983)
precedes implementation. No production unit, API/schema, dependencies, locks,
supervision policy, scientific threshold, workflow concurrency or timeout changes.

<a id="plan"></a>
## Implementation

1. Keep the existing `apps/web/scripts/native-browser.mjs` migration/application-role
   setup, original state, packaged server/dist, Caddyfile and both Playwright phases.
   Reuse its command tracking, bounds and private diagnostic redaction.
2. A thin `native-user-services.mjs` invokes native systemctl/journalctl. Copy the two
   shipped units byte-for-byte under fresh test names. Enable them with native
   `--user --runtime`; drop-ins change only executable/working-directory/environment-file
   paths. Do not modify Type, Restart/15s, stop deadline, KillMode, UMask or output policy.
3. Run both real services with the disposable application identity, original state,
   loopback origin and empty Runtime/Downstream bindings. No admin/provider credentials
   enter their environment. Observe actual native properties and `/proc` executable,
   subcommand, user, directory and the test's configuration, without publishing environment values.
4. After the first real local-session/project/receipt phase, verify no research Run exists.
   Send one unit-targeted SIGKILL to the idle Worker. Only systemd restarts it: require a
   different PID/invocation and exactly one NRestarts increment. Then stop the API normally,
   retain real gateway502/static availability, start that same unit and complete the original
   post-restart browser checks without a new login or reseeding.
5. Cleanup stops only owned units, confirms MainPID0 and empty service cgroups, collects
   bounded private logs, disables runtime links and removes owned drop-ins. Stop the gateway
   before deleting the disposable database/state. Unconfirmed shutdown retains private
   state and fails; report the actual retention flag and publish no failure screenshots.
6. Capture a unit's native ControlGroup at every show, before later process validation
   can fail, and again before stop can clear that property. On a successful main flow,
   final cleanup requires both services to exit normally; a timeout or signal is a failure.
   An already failed/interrupted flow may retain its error Result, but still must prove
   stopped/empty before deletion. Never treat a missing observation as an empty cgroup.
7. Keep the four real DDL lost-ACK/SIGTERM cleanup regressions unchanged. Eight small
   Node tests cover path formatting, real symlink targets, original-group capture,
   strict normal shutdown, failed-flow cleanup ordering, populated-group refusal and
   lost stop acknowledgement. Their scripted observations are not native systemd evidence;
   full live browser/service execution remains required. The Web workflow runs these
   early with script syntax checks and reuses the ordinary-user manager setup; full
   checks, source-unchanged assertion, existing artifact upload and timeout stay unchanged.
   No extra workflow/job.

Native interfaces: Ubuntu24.04 [systemctl](https://manpages.ubuntu.com/manpages/noble/man1/systemctl.1.html)
and [unit/drop-in paths](https://manpages.ubuntu.com/manpages/noble/man5/systemd.unit.5.html).
The native service manager owns restart and shutdown; polling observes facts and does
not implement a second recovery policy. Retain existing static installation/scope tests.

<a id="verification"></a>
## Verification

Initial Head `0291b477bd7651209665a8b5a8434c1d823b4d01` passed script syntax, Rust/API,
frontend tests/Demo and the three-viewport suite in Web35441017004, but the real
service scenario failed before unit registration. Artifact10584152255 records
`require-fresh-api-unit` exit1, then real database/role cleanup and no retained state.
Upstream systemd255 returns ENOENT for an empty filtered list. The corrected check reads
the complete native unit-file collection, requires command success, then rejects an
exact matching name; it does not ignore nonzero exits. See [the observed repair](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5741676926)
and [shutdown refinement](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5741632213).
The initial clean review5741617560 does not approve changed source or replace execution.

Head `3eb5437366cfc17259d96017032644d2ed3e0b0b`, Web35441840209/artifact10584240998,
failed unit installation validation: systemd255.4 treats the outer quotes in
WorkingDirectory as literal path characters and retained the original installation path.
API/Worker never started; native cleanup completed and retained no private state.
The [single-path correction](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5741912794)
uses the upstream v255.4 parser rules for both WorkingDirectory and EnvironmentFile:
unquoted single-line absolute paths, with literal percent signs escaped as specifiers.
ExecStart words and environment-file values keep their separate native quotation rules.
FragmentPath may name the runtime load link or its source, but realpath must identify
this fixture's original unit; the supervision policy is still checked exactly.
The new Node regressions are not a live startup pass. Current-source results belong to
[PR #95](https://github.com/zhengui666/QuaZonai/pull/95).

Require new-Head native syntax, complete Web workflow, both browser phases, actual process/property snapshots,
automatic Worker restart, graceful API stop and owned cleanup must all succeed
on the final Head. A unit file, script or configured workflow is not execution evidence.

Run `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e:native` with the
disposable PostgreSQL, real Rust/dist/Chromium and user-manager prerequisites in
CONTRIBUTING. Inspect result.json, native-user-services.json and redacted logs
alongside the exact source commit. Missing systemd, active processes after cleanup,
wrong executable/configuration, unexpected restarts or failed browser phases are failures,
not skipped passes or reasons to use direct-spawn fallbacks.

All previous Rust/Store/Runtime/science/Web/hosting/CodeQL checks remain applicable.
The scenario is same-user, runtime-only and loopback. It does not establish public
TLS, persistent installation for another account, host boot, active-Job recovery
or the complete Web/CLI research/portfolio/Paper/Forward chain. These limits do not
reopen the dedicated-account waiver or waive other #62 requirements.

<a id="review"></a>
## Independent review

Request read-only `@codex review` for each new Head. Inspect every summary/inline
finding, fix source personally and obtain actual final-Head tests. Previous clean
feedback and author inspection do not approve changed source.

<a id="delivery"></a>
## Delivery boundary

1. Complete this declared scope with committed source and actual native evidence.
2. Require every applicable final-Head CI success, all actionable findings resolved,
   and explicit clean independent Codex review for that same Head.
3. Only then mark ready and merge using expected-Head verification; inspect main,
   source identity and separately executed post-merge checks.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It reviews only.

<a id="handoff"></a>
## Continuation

Keep public turn summaries in the active PR, not product manuals. Re-read AGENTS,
CONTRIBUTING, relevant DESIGN and actual branch/check state before writing.
DEVELOPMENT.md and OpenSDLC language override are absent; preserve this English task ID.
Do not touch unrelated Dependabot work, user data, accounts, production services or
licenses. Follow the acceptance index for the remaining non-account product work.
