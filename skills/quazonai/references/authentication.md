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

## Direct-access development mode

Development/test installations may explicitly keep Operator Authentication disabled only when the complete authentication credential/origin group is absent. Production must enable authentication and provide a complete valid configuration. The Skill should still avoid assuming that direct access is available: call a harmless read such as `quazonai status`, interpret `AUTH_REQUIRED`, and require the runtime environment to supply `QUAZONAI_API_TOKEN` rather than asking for browser factors.
