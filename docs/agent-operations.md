# Connect an Agent to QuaZonai

The [quazonai skill](../skills/quazonai/SKILL.md) is for an Agent operating the running research service. Contributor instructions remain in [AGENTS](../AGENTS.md) and [CONTRIBUTING](../CONTRIBUTING.md). The [design comparison](research/service-agent-skills.md) explains the knowledge/executable boundary.

## Install the portable directory

Register or copy the entire `skills/quazonai` directory into the Agent host's supported skill location, retaining `SKILL.md` and `references/` together. Do not install just the entry file. Host-specific discovery is the host's responsibility; a file in this repository is not proof that an external Agent has loaded it. The pack has no runtime scripts or package dependencies.

Separately provide the installed QuaZonai `server` executable matching the service version, the actual local control-plane origin, explicit HTTP/CA settings when needed, and the path to an existing private machine credential. Provision credentials through the existing human-controlled service setup; never paste them into model context. A remote Agent needs its host to reach this local surface; the skill does not change the server's loopback boundary. See [operations](../OPERATIONS.md) for human setup, not as an Agent's mandatory first read.

A trusted internal Mission instead receives its original binding and MCP tools from the launcher. This installation procedure does not enable ambient skill instructions in the restricted Mission configuration, replace its native tool loop or grant CLI/Operator access.

## Native operation aids

- `server client ... identity` reads the current machine session's public binding and scopes through the existing authenticated endpoint.
- `server openapi --list-schemas` lists the installed native DTO names; `server openapi --schema ArtifactCreate` selects one DTO and all of its schema dependencies. Both are offline. The original no-argument `openapi` export is unchanged.
- `server client ... --preview ...` parses the same native request without reading credentials or sending traffic, and prints a redacted request plan. Domain checks, authority and persistence are deliberately not claimed.

Exact invocation and output semantics are in [CLI](../CLI.md); task recipes are inside the portable skill. The Agent should discover narrow schemas instead of reading the whole generated OpenAPI or application source. Permission failures do not justify a raw HTTP, shell or database fallback.
