# Buffer native market metadata reads

<a id="task"></a>
## Task

Real full-history composition spent six minutes at 98% CPU before creating output: 961,197,653 bytes read with 961,190,771 read syscalls. Both shared JSON readers passed a plain File into serde_json, causing one read syscall per byte while inspecting the 443 completed provenance units.

<a id="spec"></a>
## Change

Wrap the file in the standard 8 KiB BufReader in market-history's shared JSON loader and the cohort-cache manifest loader. Keep the existing parser, validation, metadata size limit, wire format and frame readers unchanged. No buffer configuration, parser replacement or new dependency is needed.

The running composition continues with its retained operator; this change does not restart it or modify its inputs/outputs. The complete archive and research scope remain unchanged.

<a id="verification"></a>
## Verification

Formatting and diff checks are run locally. No Cargo run or operator rebuild is planned: the owned target was reclaimed for full-data capacity. Existing native source/cache/composition tests run in current-head CI; immutable-head review and actual CI results remain pending.
