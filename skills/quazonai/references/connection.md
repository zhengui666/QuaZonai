# Connection and native discovery

## Inputs and first use

The trusted host supplies the installed QuaZonai `server` executable, the actual control-plane origin and the path to an already provisioned private machine credential file. The variables below stand for those supplied values, not guessed defaults. A remote assistant needs a host-provided connection to the local control plane; this skill does not expose it on the public network.

```sh
server --version
server client --help
server client --origin "$QZ_ORIGIN" --credential-file "$QZ_CREDENTIAL_FILE" identity
```

For an explicitly configured local HTTP deployment, add `--development-http` after `client` in each example. Never infer permission for HTTP merely from the URL. Use the exact configured `localhost` or loopback address; do not rewrite it. A private CA, when configured by the host, uses `--ca-certificate` with the supplied certificate file. Do not disable TLS validation or copy browser cookies. The CLI reads the credential file internally; do not cat it, log it or put its contents in argv.

`identity` calls the real machine-session endpoint. Check `kind`, `scope_codes`, `project_id`, `run_id` and `expires_at` against the task. A null project binding is not proof of unrestricted access. It does not grant new authority and does not list other credentials. Cache the result only for this connection/task; the service rechecks authority on each request. Revisit it after expiration, revocation, a connection change or an unexpected denial, not before every read.

When a connection is absent, return the specific missing provisioning input without asking the user to paste a token. Do not initialize state, access the database or mint a machine identity.

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
server client --origin "$QZ_ORIGIN" --credential-file "$QZ_CREDENTIAL_FILE" \
  --preview --idempotency-key "$REQUEST_KEY" artifact submit < artifact-request.json
```

Only a user-approved workspace input file belongs in this redirection. `--preview` uses the same command routing and typed JSON parser as execution. It validates the local origin, IDs, cursor/options and supplied key/grant syntax, but does not read credential/CA files or contact a server. It returns method, route, query, expected HTTP status, body byte count and required authorization indicators. Bodies, key values, grants and connection secrets are not printed. Missing idempotency keys for writes are errors; a missing Operator grant is shown as a requirement so a human can prepare authorization.

`request_sent=false`, `authorization_checked=false` and `server_state_checked=false` are intentional. Budgets, current revisions, domain eligibility, token validity and successful persistence are not established by a preview. Only remove `--preview` after checking intent and already delegated authority. The existing `migrate import --dry-run` is a different, server-side operation; it is not an offline preview and is outside this skill's maintenance scope.

## Output and pagination

Normal success is JSON on stdout and exit code 0; errors have nonzero exit and safe stderr. Do not look for Lark's `ok` or an invented universal `code=0` field in QuaZonai success responses. Commands return their native DTO or command receipt. `run watch` returns NDJSON; `artifact export` returns raw bytes and requires an exit-code check.

Lists default to a bounded page and support `--limit 1..100` and `--cursor`. Keep `next_cursor` as returned. Stop after enough evidence for the question; disclose partial coverage when a page/time bound is reached. UUIDv7 values and bigint/revision decimal strings are opaque: never round them, rewrite them or substitute display names.
