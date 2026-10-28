# Full data backup with copy-on-write

A large compressed archive made the mandatory gzip backup exceed available space on the owner's btrfs installation. Keep a complete recovery point rather than omit data or waive backup.

Probe reflink support using the completed database dump inside the new private backup directory. Where supported, copy the complete data tree with `cp -a --reflink=auto`; otherwise retain the existing tar.gz format. Restore accepts either format into its owned staging directory before the existing PG transaction, data replacement, override restoration and pending/start retry flow. No CLI option, dependency or writer-consistency change.

Validation: existing manager protocol tests cover both formats, source/restored-copy isolation, private backup permissions, failed data copies, original override handling and interrupted-start retry. Existing release unit tests and Bash syntax checks also pass. A tiny local btrfs reflink probe confirmed clone isolation. No live installation backup, 169 GB copy, service restart or Docker run was performed.
