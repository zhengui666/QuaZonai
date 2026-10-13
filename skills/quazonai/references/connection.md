# Connection and discovery

```sh
quazonai --version
quazonai login --origin https://YOUR_INSTANCE
quazonai identity
```

The user enters the password in their own terminal with hidden input. Never put it in chat, arguments, environment variables or an Agent tool call. If instance setup is required, use the authentication UI first. No private terminal means return the manual login command and stop writes.

The CLI is released and tested only on Linux x86_64. It internally reuses `client.json` with the original shape `{schema_version:1,origin,token,development_http,ca_certificate}` at `$XDG_CONFIG_HOME/quazonai/client.json` or `$HOME/.config/quazonai/client.json`, with a private 0700 directory and 0600 file. Do not read that file yourself. Legacy token bytes are preserved, but acceptance still depends on the service's retained authentication support.

`login --replace` deliberately creates a replacement device; it does not revoke the previous one. `--name NAME` labels it. `--ca-certificate FILE` uses the user's supplied CA without disabling TLS verification. An explicitly chosen HTTP origin sends credentials without TLS; the CLI never downgrades HTTPS or follows redirects.

- `logout` revokes the current device through the service, then forgets its local profile. A failed/unknown revocation retains the profile for reconciliation.
- `forget` only removes the local saved connection; it does not revoke a remote device.
- `identity` returns nonsecret session status, not provider/account quality or unrestricted authority.

```sh
quazonai schema list
quazonai schema show ResearchStart
quazonai schema show PortfolioStart
quazonai schema show ObservationStart
quazonai schema show NativeFactorEvaluationRequestV1 --domain
```

Discovery is offline and reads installed source DTOs/native schemas. The compact API source is TypeScript, not a claimed generated JSON validation schema. If schema assets are not installed, report `installed_schema_unavailable`; do not substitute retired API contracts. Help is always offline.

Success writes JSON receipts to stdout; errors use nonzero exit and sanitized JSON on stderr. Watch emits NDJSON snapshots and an observation result. Lists return real arrays/pages; preserve an opaque research `cursor` unchanged and disclose partial coverage. No request is automatically retried and no key is generated for you.
