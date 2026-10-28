# Reuse the installed official Node runtime for CLI packaging

Release 40638266303 failed its macOS arm64 CLI job after a redundant download of the official Node 22.19.0 archive timed out repeatedly and returned an HTTP/2 error. The matrix had already installed the same pinned platform runtime using official setup-node.

Pass the installed runtime root derived from `process.execPath` to the existing packer. Keep the official executable, LICENSE, archive inspection, extracted CLI execution and platform installer checks. Remove the second network download and unused archive platform labels; no retries, mirrors, dependencies or release checks are added.

Official provenance: setup-node at the pinned revision caches the full extracted distribution and adds its bin directory (Unix) or root (Windows) to PATH. actions/node-versions builders fetch archives from nodejs.org/download/release and retain their complete contents, including the Node LICENSE.

- https://github.com/actions/setup-node/blob/49933ea5288caeca8642d1e84afbd3f7d6820020/src/distributions/base-distribution.ts
- https://github.com/actions/node-versions/blob/main/builders/nix-node-builder.psm1
- https://github.com/actions/node-versions/blob/main/builders/win-node-builder.psm1
- https://github.com/actions/node-versions/blob/main/installers/nix-setup-template.sh

Local validation uses the existing packaging tests and an actual pack/extracted-CLI round trip with the previously published official Node 22.19.0 executable and its LICENSE in a private temporary directory. No Cargo, live update or full release publication was run locally. The existing release matrix retains actual platform packaging and installer execution.
