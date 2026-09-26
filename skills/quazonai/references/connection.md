# Connection and native discovery

## Inputs and first use

Use the installed QuaZonai `server` executable. The owner logs in once on the machine that will run the external Agent:

```sh
server --version
server client --help
server client login
server client identity
```

`login` asks for the QuaZonai frontend address (the same URL used in the browser, without a path) and the instance password with hidden terminal input. First set the password in the frontend if the instance is new. **The user types the password directly in their own terminal. Never ask them to paste it into chat, pass it in argv/environment, pipe it through the Agent, or read a password/profile/token file.** If an Agent cannot offer a private user-controlled terminal, return this one manual login command and resume after the user completes it.

The CLI saves only the instance address, private device token and transport settings in `$XDG_CONFIG_HOME/quazonai/client.json` (or `$HOME/.config/quazonai/client.json`). Subsequent `server client` commands reuse that connection without `--origin`, `--credential-file`, a browser session or an Operator grant. The connection has no automatic expiry. Repeating `server client login` confirms the existing connection without creating another device. To intentionally switch instance, replace the device name or recover an unreadable/obsolete saved profile, the user runs `server client login --replace`; the previous device remains listed in its instance's authentication settings until deleted. The owner can name it with `server client login --name NAME`; otherwise the native hostname identifies it. In the frontend, **Settings → Authentication** can change the password or delete any connected CLI machine. A password change ends browser sessions but keeps connected CLI machines until explicitly deleted. After revocation, stop and have the user log in again in their own terminal.

HTTPS supports the actual public frontend hostname. For an explicitly configured local HTTP deployment, the user runs `server client --development-http login`; HTTP remains loopback-only and this choice is saved. Never infer HTTP permission from the URL. A host-provided private CA uses `server client --ca-certificate /supplied/ca.pem login` and is saved with the connection. To update its CA later, use `server client --ca-certificate /supplied/new-ca.pem login`; successful identity confirmation saves the new certificate path for subsequent commands. Do not disable TLS verification, copy browser cookies or rewrite the origin.

`identity` returns only the current device's public ID, name, creation time and last-use time. Check that it is the intended connection. The service rechecks device revocation on every request; successful login does not authorize operations outside the user's task.

### Existing scoped machine connections

A trusted host may still supply a restricted machine credential for a specific project, Reviewer, Automation or Downstream role:

```sh
server client --origin "$QZ_ORIGIN" --credential-file "$QZ_CREDENTIAL_FILE" identity
```

Keep these explicit connection flags paired. The CLI reads the private credential file internally. For a scoped identity, check `kind`, `scope_codes`, `project_id`, `run_id` and `expires_at` against the task; a null project binding is not proof of unrestricted access. Scoped credentials still require an exact Operator grant where indicated. Never replace a bound Mission or restricted credential with the owner's saved login to bypass a denial. A bound Mission always uses its original MCP connection, never this CLI login flow.

## Discover fields, not source files

```sh
server openapi --list-schemas
server openapi --schema ArtifactCreate
server openapi --schema ExperimentProposalV1
```

Listing schema names is offline. A selected schema result contains `schema_version`, `name`, an entry `schema` reference and `components.schemas` with its transitive native references. Use that complete closure, including required fields and string/number distinctions. These are the installed binary's Rust contracts, not a live-server compatibility check or an authorization catalog. Use a single DTO at a time; do not load the full export unless explicitly needed.

The unmodified `server openapi` still exports the full native document. Unknown names fail rather than returning a guessed structure. Missing flags indicate an older client; stop and report the installed version instead of compiling or downloading code.

## Preview without acting

```sh
server client --preview --idempotency-key "$REQUEST_KEY" artifact submit < artifact-request.json
```

Only a user-approved workspace input file belongs in this redirection. `--preview` uses the same command routing and typed JSON parser as execution. It validates the local origin, IDs, cursor/options and supplied key/grant syntax, but never contacts a server or opens CA files. Saved-connection previews read the private profile internally to select the device identity; explicit scoped previews do not open the credential file. It returns method, route, query, expected HTTP status, body byte count and required authorization indicators. Bodies, key values, grants and connection secrets are not printed. Missing idempotency keys for writes are errors; saved owner devices do not require Operator grants, while a missing grant for an explicit scoped credential is shown as a requirement so the authorized human can prepare it.

`request_sent=false`, `authorization_checked=false` and `server_state_checked=false` are intentional. Budgets, current revisions, domain eligibility, token validity and successful persistence are not established by a preview. Only remove `--preview` after checking intent and already delegated authority. The existing `migrate import --dry-run` is a different, server-side operation; it is not an offline preview and is outside this skill's maintenance scope.

## Output and pagination

Normal success is JSON on stdout and exit code 0; errors have nonzero exit and safe stderr. Do not look for Lark's `ok` or an invented universal `code=0` field in QuaZonai success responses. Commands return their native DTO or command receipt. `run watch` returns NDJSON; `artifact export` returns raw bytes and requires an exit-code check.

Lists default to a bounded page and support `--limit 1..100` and `--cursor`. Keep `next_cursor` as returned. Stop after enough evidence for the question; disclose partial coverage when a page/time bound is reached. UUIDv7 values and bigint/revision decimal strings are opaque: never round them, rewrite them or substitute display names.
