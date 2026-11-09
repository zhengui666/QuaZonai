# Internal computation recovery acceptance

`p0-environment.json` records actual probes against the managed execution host at
source `8fe4f9d25dc0a66e32237edf0e9f7db5ad51e227`. The result is **BLOCKED**. No
scientific command is verified by that report.

Official runC 1.5.2 and CRIU 4.3 were obtained into task-owned scratch. The
host-side probe fails because the ordinary task user lacks the required kernel
capabilities. A rootless user-namespace probe also fails kernel feature
detection. A separately approved, task-owned, network-disabled privileged Docker
container was then actually tested; that probe still fails because socket
diagnostics and `/proc/sys/kernel/ns_last_pid` are unavailable. No host settings,
kernel modules, other containers or user databases were changed.

The preexisting native `target/debug/job` belongs to the old main checkout. It
was not used to claim current-dev scientific acceptance. Rust 1.99.1 and Cargo
were not on the task user's PATH. No Nautilus/Clarabel/Wasmi internal continuation
or power-loss acceptance is claimed.

Run the required preflight on the intended Linux computation node:

```sh
QZ_RUNC_BINARY=/absolute/official/runc \
QZ_CRIU_BINARY=/absolute/official/criu \
node --experimental-strip-types \
  apps/nautilus/test/recovery-native-preflight.ts --output /private/p0-preflight.json
```

Missing prerequisites fail with a nonzero exit status. A successful preflight is
only `PREFLIGHT_ONLY`; it never writes a verified-command profile. Actual native
acceptance must additionally run the matching scientific executables and frozen
fixtures through in-progress checkpoint/restore, prove continuation from the
saved frontier, compare complete scientific results, and retain external
observations outside the rolled-back work directory. Normal unit tests and
process-kill tests cannot substitute for those checks or for VM/block-device
power-loss testing.
