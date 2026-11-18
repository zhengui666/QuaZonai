# Connection, output, and errors

## CLI and instance

Use the existing `quazonai` executable. Release CLI bundles include Node; running the JavaScript package directly requires Node 22.19 or later.

If the executable is missing, use the official installer from the project's release. For client-only installation on Linux/macOS, run `bash install.sh --cli-only`. The default Linux path also installs or updates the service stack, so do not omit this option. Windows x86_64 uses the release's `install.ps1`. Then rerun `quazonai --version` and `quazonai --help`. Do not assume a published npm package or substitute a source build for the requested client installation.

Run `quazonai identity` first. A valid session returns `authenticated: true`. An unauthenticated response may still be JSON but exits with `1`.

For initial login, the user runs this in their own terminal:

```sh
quazonai login --origin https://qz.example.com --name my-agent
```

Replace the example domain with the actual instance origin: scheme, host, and optional port only, without `/api/v3`, username, password, query, or fragment. The CLI adds `/api/v3`.

- Password input is hidden in an interactive TTY. Do not request a password in chat or pass it through a stdin pipe or command-line argument.
- `login` reuses a valid current session. To switch instance or login, use `login --origin URL --replace`. An ordinary command's `--origin` cannot switch saved configuration.
- Use `--ca-certificate /absolute/path/ca.pem` for a custom CA; the path saved at login can be reused. Do not disable TLS validation.
- `set_instance_password_in_auth_ui` means the user must first set a password through the instance authentication page.
- For `login_requires_user_terminal` in a non-TTY agent environment, give the command above, wait for the user to complete it in the same execution environment, then rerun `identity`.

Configuration is stored at `$XDG_CONFIG_HOME/quazonai/client.json` on Unix, or `~/.config/quazonai/client.json` if XDG is unset; Windows uses `%APPDATA%\quazonai\client.json`. It contains a device token. Do not read, copy, or print it for ordinary business operations.

| Command | Result |
| --- | --- |
| `logout` | Revoke the current remote device credential, then forget local configuration on success |
| `forget` | Remove only local configuration; returns `remoteRevocationRequested: false` |

If `logout` fails, do not present `forget` as credential revocation.

## Output and exit codes

| Situation | Exit code | Interpretation |
| --- | --- | --- |
| Ordinary command success | `0` | Read JSON business state; a submission receipt only establishes acceptance |
| Input, authentication, HTTP, or connection failure | `1` | Read stderr `{error, status?}` |
| Unauthenticated `identity` | `1` | Read session fields on stdout |
| Research observes submission `done` | `0` | `settled: true`; still inspect research results |
| Research observes submission `unanswered` | `2` | Settled but not a successful submission; inspect diagnostics |
| Portfolio observes `COMPUTED` | `0` | `settled: true`; inspect metrics, limitations, and capacity |
| Portfolio observes `FAILED` / `STOPPED` | `2` | Read business errors and existing results |
| `wait` observation timeout | `124` | `settled: false`, `cancellationRequested: false` |
| `watch` observation timeout | `0` | May also be `settled: false`; do not rely solely on exit code |
| SIGINT/SIGTERM detaches `watch` / `wait` | `130` | Does not mean remote work was canceled |

Parse `watch` output line by line, not as one JSON object. Ordinary requests have no default network timeout. Externally terminating a write command can leave its outcome unknown; handle it as an unknown write.

## Error recovery

| Error or state | Action |
| --- | --- |
| `login_required` | Have the user log in interactively, then read identity |
| `login_replace_required` | Check the target instance; use `--replace` when a replacement is intended |
| `client_profile_invalid` | Retain the error; if a reset is needed, use `forget` then login rather than manually editing tokens |
| `invalid_origin` | Use an origin without a path, embedded credentials, or query |
| `service_unavailable` / `invalid_response` | Check connectivity, then retry reads; do not recreate business objects |
| `event_stream_unavailable` / `invalid_event_stream` | `get` the same ID; resume `watch` / `wait` after reconnection |
| `write_outcome_unknown_retain_original_request` | Preserve the file and ID, query first; never automatically retry with a new request |
| `request_id_conflict` | The ID was used with different content; recover the original request or explicitly start a new operation |
| `mandate_revision_conflict` / `capital_exit_revision_conflict` | Read the owning object's revision and reassess; do not blindly increment |
| `file_required` / `input_file_unreadable` / `input_json_invalid` | Use an existing readable JSON object file, not an array, bare string, or stdin `-` |
| `invalid_json_number_use_decimal_string` | Represent large integers/decimals as contract-required strings without losing precision |
| `schema_not_found` | Choose a name from the same version's `schema list` or `schema list --domain` |
| `installed_schema_unavailable` | Report missing contract assets and repair the client installation; do not guess request shapes |
| `404` | Check instance and ID; hidden deleted resources may also be unreadable |
| `409`, business blockers, `UNSUPPORTED` / `BLOCKED` | Read reasons and current state; do not loop submissions or broaden scope to bypass limits |

For initial instance setup, model credentials, account connection creation, or data preparation not available through the CLI, identify the missing prerequisite and point to the existing instance UI. Do not invent `connections`, `models login`, `datasets import`, or `request` subcommands.

For an exited native call still marked `UNKNOWN`, use only the existing source-review settlement interface with original task identities and diagnostic evidence. A nonzero exit alone does not authorize retry. `QUERY_PLANNING_TYPE_COERCION_REJECTED` recognizes only the supported exact query-planning diagnostic; establish execution phase from original entrypoint source and records. Owner install may already have written a pending header, and earlier catalogs may have read metadata or prefetched. Settlement does not roll back these side effects, treat a header as a completed report, or rewrite the original terminal Task or stderr. SQL correction belongs to the research strategy; the product does not globally rewrite filters.

`DELTA_PRECISION_REJECTED_DURING_REPLAY` recognizes only the exact order-book Add/Update price or quantity precision mismatch from an exited original configured backtest. It is a runtime failure, not pre-execution rejection: some market data and Actor callbacks may already have run and produced pending headers, observations, and STOP_NON_EOF. Settlement retains all partial output, the original UNKNOWN Task, and stderr. It neither rolls back arbitrary owner side effects nor automatically replays or promotes partial output to a completed report. Changing data/definition precision is a separate explicit step; settlement must not silently round or overwrite the original catalog.
