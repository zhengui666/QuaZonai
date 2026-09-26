# Connection and native discovery

## Inputs and first use

Use the installed `quazonai` executable. The owner logs in once on the external Agent's machine:

```sh
quazonai --version
quazonai client --help
quazonai client login
quazonai client identity
```

`login` asks for the frontend origin used in the browser and the instance password with hidden input. A new instance first needs its password set in the frontend. **The user types the password directly in their own terminal.** Never collect it through chat, argv, environment or Agent input, or read password/profile/token files. Without a private user-controlled terminal, return this manual command and resume after login.

The CLI privately saves the origin, device token and transport settings in `$XDG_CONFIG_HOME/quazonai/client.json` (otherwise `$HOME/.config/quazonai/client.json`). Later commands reuse them without connection flags, browser sessions or Operator grants. Devices do not expire automatically. Repeating login confirms the connection; `login --replace` intentionally replaces it, and `login --name NAME` supplies a label instead of the hostname. Replacement leaves the previous device registered until deleted. **Settings → Authentication** manages devices and the password; changing the password ends browser sessions but preserves devices. A revoked device requires another user-controlled login.

Use the actual public HTTPS hostname. Only an explicitly configured loopback HTTP deployment uses `quazonai client --development-http login`. A supplied private CA uses `quazonai client --ca-certificate /supplied/ca.pem login`; successful login or identity confirmation saves that path, including a later CA replacement. Never infer HTTP permission, disable TLS verification, copy browser cookies or rewrite the origin.

`identity` returns nonsecret device metadata. Verify the intended connection; the service rechecks revocation on every request.

### Existing scoped machine connections

A trusted host may still supply a restricted machine credential for a specific project, Reviewer, Automation or Downstream role:

```sh
quazonai client --origin "$QZ_ORIGIN" --credential-file "$QZ_CREDENTIAL_FILE" identity
```

Keep these flags paired; the CLI reads the credential internally. Check `kind`, `scope_codes`, `project_id`, `run_id` and `expires_at`; a null project is not unrestricted authority. Scoped credentials still require an exact Operator grant where indicated. A bound Mission uses its original MCP connection, never this CLI login flow.

## Discover fields, not source files

```sh
quazonai openapi --list-schemas
quazonai openapi --schema ArtifactCreate
quazonai openapi --schema ExperimentProposalV1
```

Discovery is offline. A selected result contains its entry `schema` and transitive `components.schemas`; honor required fields and string/number distinctions. It describes the installed Rust contracts, not live-server compatibility or authority.

`quazonai openapi` exports the full document only when needed. Unknown schemas fail. Missing commands or flags require reporting the installed version/capability gap.

## Optional local preview

Authorized routine writes may execute directly. Preview complex new requests, requested inspections, destructive operations and requests awaiting human review:

```sh
quazonai client --preview --idempotency-key "$REQUEST_KEY" artifact submit < artifact-request.json
```

Use an authorized workspace input. `--preview` shares execution's routing and typed parser, validating local arguments without contacting the server or opening CA/explicit credential files. Saved profiles are read internally. The result describes the route, status, body size and authorization requirements while redacting bodies, keys and secrets. Writes still require an idempotency key; scoped grant requirements are reported without issuing a grant.

`request_sent=false`, `authorization_checked=false` and `server_state_checked=false` mean no server outcome was checked. Execute the unchanged request only within existing authority. `migrate import --dry-run` instead contacts the server and is outside this Skill's maintenance scope.

Forward submissions use `quazonai client forward weights submit` (`DownstreamWeightsSubmitV1`) and `quazonai client forward messages submit` (`ForwardMessageSubmitV1`) with the original Downstream authority. Read snapshots with `forward weights list PROJECT_ID`. Legacy `forward-weights`, `forward submit` and `forward weights PROJECT_ID` remain compatible, but use grouped spellings for new commands.

## Output and pagination

Success is the native DTO/receipt on stdout with exit 0, not a universal `ok`/`code` wrapper. Errors use nonzero exit and safe stderr. `run watch` emits NDJSON; `artifact export` emits raw bytes and requires an exit-code check.

Lists support `--limit 1..100` and unchanged `--cursor` values. Stop when enough evidence is available and disclose partial coverage. UUIDv7 and bigint/revision strings are opaque; do not round or replace them with names.
