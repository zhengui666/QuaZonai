# Operator authentication for the QuaZonai CLI

QuaZonai can protect its single-operator Web/API surface with password + RFC 6238 TOTP browser login. The CLI does **not** perform that interactive browser flow.

## Machine credential

When Operator Authentication is enabled, provide the same machine token configured on the Core API through the CLI process environment:

```bash
export QUAZONAI_API_TOKEN='<machine token from the QuaZonai deployment secret store>'
quazonai status
```

The CLI sends it as `Authorization: Bearer ...` to operator-owned `/api/v1` resources. Treat this environment value as a secret:

- source it from the operator's shell secret manager or protected service environment;
- do not put it in commands, screenshots, issue text, logs, examples, artifacts, or repository files;
- rotate it in the API deployment and all legitimate automation consumers together;
- a missing, malformed, or incorrect token returns `AUTH_REQUIRED`; fix credential injection before retrying a mutation.

## Identity boundaries

Three credentials are intentionally non-interchangeable:

1. **Browser operator credential** — password + TOTP produces HttpOnly session/trusted-browser cookies. The CLI never reads or stores the password, TOTP setup secret, one-time code, or browser cookies.
2. **Machine operator credential** — `QUAZONAI_API_TOKEN` authorizes the local CLI/automation against operator-owned API resources. It is not returned to the browser.
3. **Downstream service credential** — each Downstream System has its own one-time-issued service token for its Handoff `claim`, `accept`, `reject`, package download, and feedback operations. The machine operator token cannot replace it.

Never copy one credential into another field or retry an authorization failure with a different identity class.

## Explicit opt-in and direct access

`QUAZONAI_AUTH_ENABLED` is the only switch for QuaZonai Operator Authentication. When it is `false`, direct Web/operator API access is preserved in every environment, including production, and dormant authentication credential/TTL values do not implicitly enable or validate the feature. Such a deployment should remain loopback-only or behind another deliberately trusted access boundary.

When `QUAZONAI_AUTH_ENABLED=true`, the complete username/password/TOTP/cookie-key/machine-token/public-origin configuration is required and invalid configuration fails closed; enabled production authentication additionally requires HTTPS. The Skill should not assume which deployment choice was made: call a harmless read such as `quazonai status`, interpret `AUTH_REQUIRED`, and require the runtime environment to supply `QUAZONAI_API_TOKEN` rather than asking for browser factors.
