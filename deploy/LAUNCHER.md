# QuaZonai standalone launcher

This separate portable distribution contains QZ glue and the official prebuilt
Node runtime. It does not contain NautilusTrader, a Rust compiler, a strategy,
wallet credentials or a native owner binary. Unpack and run `quazonai-launcher`
(`quazonai-launcher.cmd` on Windows); no npm/Cargo installation or local build.
The glue is JavaScript/TypeScript executed by the bundled runtime, not a universal
precompiled Nautilus engine.

## Compatibility and prerequisites

The execution host must already have a trusted, compiled owner implementing
`quazonai-native-process-v1`, the declared identity/capability handshake and the
account-runtime business protocol. Exact Nautilus source/version/revision,
precision and job version must match the explicit expected identity; that is a
compatibility declaration, not binary attestation or universal ABI compatibility.
Only strategies already compiled and registered in that owner are available.
Installing a bare Nautilus library does not supply this interface. This tool
cannot retrofit or hot-attach an existing process without it.

Linux x86_64 supports local `serve` and `relay` over a private Unix socket.
Linux/macOS/Windows packaged clients support `inspect`, `hello` and `request`
over an existing SSH client and explicit pinned known-hosts file. No SSH keys,
host trust, account configuration, listener or service are installed automatically.
A trusted remote executable may itself have side effects; inspect only owners
you intend to execute. No Rust source or private strategy is uploaded by this tool.

## Commands

- `inspect --file CONFIG.json`: same fixed SSH identity inspection as
  `quazonai remote-owner inspect`, not a connection or trading proof.
- `serve --file CONFIG.json`: same existing-owner configuration as
  `quazonai remote-account serve`. Requires an existing saved account directory,
  owner executable and exact expected identity; does not create/start a strategy.
- `hello --file TRANSPORT.json`: inspect the running supervisor's exact binding.
- `request --file ORIGINAL_ENVELOPE.json`: send one allowlisted business request
  with the exact original request and binding. Explicit START can trade; this
  command is not restricted to observation. Do not put private keys in envelopes.
- `relay --socket-path PRIVATE_SOCKET`: the fixed SSH-side relay command.

SSH transport `relayExecutable` can point to `quazonai-launcher` or the QZ CLI.
The launcher accepts the existing fixed `remote-account relay` invocation as a
wire alias, preserving one protocol rather than introducing another connection.

Disconnect/timeout yields UNKNOWN, never automatic resend or owner shutdown.
Signals and controller death do not prove that an owner stopped or survived.
An orphan retaining the receipt lease blocks a replacement; there is no hot
recovery of lost stdin. Connection/RECOVERY_STATUS ready is not proof of complete
exchange-history reconciliation. Preserve old requests and UNKNOWN. No strategy
or live account is started merely by unpacking, inspecting or serving.
