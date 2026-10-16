# Bound the Container host ripgrep setup

## Failure and scope

The [Container job for c170cdc2](https://github.com/zhengui666/QuaZonai/actions/runs/36884141745/job/110443030265)
passed its release-helper tests, then spent the remainder of its 75-minute run
waiting for an Ubuntu package index mirror while installing missing host `rg`.
The application build never started. This change replaces only that prerequisite
installation; the real image, installation, upgrade and recovery checks still run.

## Official input and behavior

[The helper](../../../deploy/docker/setup_ripgrep.py) always installs
`ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz` from the
[official release](https://github.com/BurntSushi/ripgrep/releases/tag/14.1.1).
The [official checksum asset](https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz.sha256)
was retrieved over HTTPS on 2026-10-01 and contains SHA256
`4cf9f2741e6c465ffdb7c26f38056a59e2a2544b51f7cc128ef28337eeae4d8e`.
The version, URL and digest are fixed in source, with no runtime override.
No upstream implementation is copied into this repository.

Setup supports the Container callers' Linux x86_64 hosts. It ignores existing
`PATH` copies of `rg`, whose version and provenance are not established. It uses
a fresh private directory under `RUNNER_TEMP`, leaves existing tools untouched,
and appends the new directory to `GITHUB_PATH` only after verification. Later
steps therefore use the pinned binary. No sudo or package-index operation is
needed for this prerequisite.

The single HTTPS transfer has a 10-second connection limit, 60-second total
limit, three redirects at most, no retries and a 4 MiB byte cap. Python also
kills and waits for the download process after 65 seconds. HTTPS-only redirects
and normal certificate validation remain enabled; user curl configuration is
disabled. curl 8.4 or newer is required because
[earlier curl versions](https://curl.se/docs/manpage.html#--max-filesize) do not
enforce the size cap when the server omits its content length.

After the pinned digest matches, exactly one expected regular `rg` member is
copied, with a 16 MiB binary limit. Archive paths, links, ownership and permissions
are never extracted. The executable must report exact version `14.1.1` within
five seconds. Any failure removes only the newly owned directory and fails the
prerequisite step; there is no package-manager or unverified-binary fallback.
This bounds prerequisite setup, not the complete Container build duration.

## Verification

[Focused offline tests](../../../deploy/docker/setup_ripgrep_test.py) cover
successful installation, existing-tool preservation, independent retries, hash
mismatch before archive parsing, missing/duplicate/unsafe members, archive and
binary byte limits, partial network failures, TLS/HTTP failures, actual download
timeout kill/reap, old curl, wrong or stalled binary version, and failed path
publication. Fixtures do not represent application or Agent evaluation evidence.

All 14 focused tests passed. Both default and `RELEASE_BRANCH=dev` deployment
helper discoveries ran 164 tests: 163 passed and the existing legacy Codex
installation test could not create an AF_UNIX socket in this executor (`EPERM`).
Neither discovery is reported as a full pass. A real HTTPS download through the
helper matched the pinned digest, reported `ripgrep 14.1.1 (rev 4649aa9700)`, and
passed a fixed-string search with an exact expected line number. Its temporary
archive and executable were removed afterward. No Docker build, hosted CI rerun
or model evaluation was executed for this change.

Container action YAML parsing, every embedded Bash step's syntax, new local
Markdown links and `git diff --check` passed. The full `make check-links` could
not run because this executor has no `lychee`; Rust-backed documentation checks
were not run under the no-build resource constraint.

Independent native review and applicable CI on the final published Head remain
required by [review](../../review.md). This task does not change source
instructions, evaluation configuration, cache controls or acceptance gates.
