# Independent review and resulting decisions

Reviewed 2026-09-13 by three delegated agents: coverage/product support,
workload/fixture arithmetic, and environment/verification contracts. Root
consolidated their findings. Reviews were read-only; no product builds or
benchmark sampling, and no edits to v0.1.5 release work.

## Existing coverage inspected

| Existing family / source | Finding | v0.1.6 decision |
| --- | --- | --- |
| dedup_branch_history | Distributed/hotset/recurring/metadata/unrelated at1/10/100/500; ordinary edited files≤48KiB | Reuse current10/100 small-file cases; add large, boundary and namespace/inode histories |
| dedup_workspace_reuse | Exact/local/unique additions inside one workspace, mostly1MiB files | Do not mistake this for concurrent workspace coverage |
| dedup_cross_file / dedup_cdc_locality | Strong1MiB sharing/locality cases; boundary proof through32769B | Add explicit131071/131072/131073 transitions |
| workspace_change_locality | mixed-v4:100MiB/2000 files with50MiB anchor;500MiB/5000 files with300+100MiB anchors | New decimal caps and many1MiB files, retain old fixture identities |
| mixed_load_bearing | High tiers share64MiB small-file background +500 episode cells; tier scales episodes, not bytes; all initial files≤48KiB | Add actual L100/L500 mixed-development profiles |
| tiny_file_churn | Bulk mixed-v3 has workload100/500MiB plus1MiB/200-file witness | Count witnesses and transient names inside new caps |
| init_namespace | 300MB/10k,500MB/100k small-heavy fixtures with100MB anchors | Reuse deterministic manifest/qualification ideas, not their cardinality |
| workspace_reliability | Existing aliases, open-rename-unlink, symlinks, chmod/mtime, dirty-net-zero, lease/failure proofs | Reuse real supported semantics in M1; don't create duplicate reliability framework |
| historical_access / repository_history | Existing bounded access and explicit optional real histories | Extend access with exact transition ordinals; keep real history optional |

## Accepted improvements

1. **Reserve names and bytes.** Initial4,984/29,984 regular paths and cap−65,536B
   leave16 names for alias/symlink/temp-file operations. Actual M1 high water is
   initial+13 names/+37,120B. The fixed cap counts alias logical-path bytes too.
2. **Every requested commit must be Created.** Repeated chmod, net-zero writes
   or same-state subtree recreation can return UpToDate. Toggle metadata and
   retain explicit unique-content cohorts; count net-zero probes honestly.
3. **Real inode difficulty.** Persist aliases in stage4; stage5 replaces names
   while old aliases/descriptors retain old inodes; test open-unlinked access.
   Add alias-aware boundary roundtrip as the seventh boundary case.
4. **Variance without a Cartesian explosion.** Structured/random content by
   deterministic ordinal, exact/local/unique content cohorts, hot/rotating
   refresh cohorts, alternating directory names and head/middle/tail edits.
   K10 is a prefix of K100; source arms use identical fixtures/schedules.
5. **One shared resource budget.** Two/four mounts in one standard2CPU/2GiB
   container, shared host Store. New coordinator still requires qualification.
6. **One lease per branch.** Multiple live workspaces use distinct branches;
   second same-branch lease rejection is inherited correctness coverage.
7. **Supported attributes only.** chmod/mtime/truncate are positive coverage;
   existing setxattr expects EOPNOTSUPP, not persistence. No new chown/ACL/atime
   claim. Numeric inode identities are not stable across remounts.
8. **No persistent helper assumption.** Keep sessions/daemon alive but finish
   each helper and writable handle before Commit; preserve public fences.
9. **Honest proof scope.** Every parent edge and changed path; bounded large
   ranges in regular load verification; full byte/state proofs explicit.
10. **Fixed work.** K10/K100 stay regular targets; optional400-commit pressure
    and exhaustive101-state checks are finite work, not a timed soak.
11. **Same-file SDK batching.** Source review caught that the public batch API
    rejects cross-file members. M1, compact inode history and boundary exchange
    therefore use two ordered single-file calls; no API expansion or false
    cross-file atomicity claim. Counters reflect two calls.
12. **Recurrence really recurs.** Refresh cohorts contain1,024 files (16×64);
    the first two cycles revisit cohort0. Initial A and per-role visit parity
    prevent accidental unique-only data or repeated no-op writes. Compact
    branch controls continue the inherited ordinal and compare file-content
    IDs rather than assuming automatic timestamps/whole roots are equal.

Rejected: adding a ten-minute endurance duration, multiplying container
resources by worker count, silently reducing K100 after a miss, claiming
compaction after its removal, and assuming repeated writes always add commits.

## Source references

- [Branch history definitions](../../../../benchmark/fs-bench-pro/families/dedup_branch_history/mod.rs)
- [Workspace reuse definitions](../../../../benchmark/fs-bench-pro/families/dedup_workspace_reuse/mod.rs)
- [Shared fixture and inode oracle](../../../../benchmark/fs-bench-pro/workload/workspace_common.rs)
- [Ordinary episode implementation](../../../../benchmark/fs-bench-pro/workload/ordinary_workloads.rs)
- [Reliability POSIX operations](../../../../benchmark/fs-bench-pro/workload/reliability_workloads.rs)
- [Reliability coordinator](../../../../benchmark/fs-bench-pro/src/workspace_reliability.rs)
- [Runtime resource defaults](../../../../benchmark/fs-bench-pro/shared/runtime.py)
- [Historical-access deadline](../../../../benchmark/fs-bench-pro/shared/historical_access.py)
- [Public SDK](../../../../crates/layerfs-sdk/src/client.rs)
- [Branch ancestry](../../../../crates/layerfs-layerstack-store/src/branch.rs)
- [Branch leases](../../../../crates/layerfs-layerstack-store/src/schema.rs)
- [Daemon multiple mounts](../../../../crates/layerfs-daemon/src/main.rs)
- [Content threshold](../../../../crates/layerfs-content/src/file/content.rs)

Numerical runtime feasibility, reference-machine fingerprint and exact source
arms remain prerequisite work. [Issue #122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122)
now binds all six families and requests their implementation and qualification.
These reviews establish design coverage and source support, not performance or
release acceptance.
