# #241 Phase 1B service.finish diagnostic contract

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before adding finish instrumentation or taking a diagnostic attempt.
> The retained v4 performance receipts and classifications are unchanged.

Diagnostic identity: `issue241-phase1b-finish-lft1-v1`. Source parent
`469bc728b819456c3f06e6ca49f71f5898487105`; the instrumented product
source and its locked release host/daemon/editor/verifier binaries, exact
compilation seal, image ID, kernel/fuser, harness hash and prepared-master
hashes must be sealed in an append-only pre-run manifest before the first
live attempt. A source or harness change requires a new identity. The frozen
[#241 v4 insert registry](../../../../../benchmark/fs-bench-pro/registry/workspace-exec-insert-v4.json)
has SHA-256 `9decc295b688083db7b1f6899ebbc315eeeddfa28d0fafe68537507d12394fbb`.
The existing editor sources are `workload/src/splice.rs` SHA-256
`e765fbf720a59052077b70eaf5405984c3157beb89d8a9b9e3c41e4bcd7e1062`
and `workload/src/main.rs` SHA-256
`6340cd3bea195734ab2ef07b526d0c2a6a09d74d685847a584dc4a61ca08ca5a`.
The exact executed tool binary and image are not known before their build;
neither receives an invented digest. `core/Cargo.lock` SHA-256 at this
contract is `a370d4f7beacef2fe274bb432fadc86052e5bd30a2bd2365afb453f3c7b43855`.

The two declared shapes are one **1 MiB** pristine file, middle insertion at
524,288, and one **capped-500 MiB** pristine file of 524,283,904 bytes,
insertion at 262,141,952. Both use the v4 sealed 4,096-byte Bytes payload,
delete length 0, exact v4 `splice --output-version 4` command, two STATE
callbacks, one EDIT callback, one Workspace revision, 4,096 accepted literal
bytes and zero shifted suffix bytes. Both routes are one public SDK
`WorkspaceApi::exec` followed by public `WorkspaceApi::commit`; Project,
Sandbox, Mount, Status, Unmount and Sandbox Delete also use public SDK.
Independent writable byte copies of validated closed prepared masters are
setup only. The operation timer starts immediately before Exec and ends at
the typed Commit result. Its wall/CPU/RSS evidence comes only from LFT1.

Add bounded LFT1 children within the existing `service.finish` parent:
`storage.finish.drain`, `storage.finish.owner`, and beneath owner
`storage.finish.pack_seal`, `storage.finish.transaction_begin`,
`storage.finish.candidate_flush`, `storage.finish.ownership_publish`,
`storage.finish.ordinal_watermark`, `storage.finish.sqlite_commit`,
`storage.finish.pool_clone` and `storage.finish.candidates_clone`. Each
named child occurs at most once in a finish call. A skipped step has no
fabricated duration; a failed step retains its completed parent/children.
Report parent wall minus the union of disjoint immediate children as residual,
and owner wall minus its disjoint children as residual. CPU/RSS coverage must
be labelled from actual LFT1 sample windows, never filled from a process
clock or lifetime peak.

The count receipt records the save's inserted/reused/full/prefix objects,
pack bodies and bytes, `objects` INSERT statements, transactions/commits,
content-candidate and pooled-index entries/live bytes before their separate
postcommit clones, pooled pack fetches/bytes and chain object/encoded bytes.
Retain SQLite page size and page-count observations with their read boundary;
do not attribute a post-run page count to `service.finish` alone. On Linux,
retain `/proc/<service-pid>/io` `read_bytes` before and after the diagnostic
command as a **process-wide physical-read observation**; concurrent Service
work or unavailable counters make substep attribution unavailable rather
than zero. No unsafe SQLite FFI or third-party package change is authorized.

Run exactly one labelled diagnostic per shape at the frozen instrumented
identity, each in a fresh append-only directory under this folder. Each
complete functional/performance command must be ≤15 s with one construction
worker; each independent identity-matched verifier is under 10 s and public
Sandbox Delete is confirmed. Keep raw caller, daemon and Service LFT1,
callback/status counts, external process I/O counters, build/image/fixture
seals, complete command and verifier walls, every error and cleanup result.
The Linux FUSE backing cache may serve Commit from Edit's resident writes;
until invalidation and full residency checking are proven, raw timings are
`INELIGIBLE` for cold latency admission. These diagnostics are **not** a
repeat of the historical v4 performance arm and cannot replace its receipt.
If counts do not isolate a safe causal fix, record that decision and do not
resample the unchanged four cases. If they do, freeze the changed-source
four-case selection and prospective raw aims of 55/55/60/70 ms plus
≤15 ms 500-minus-1 MiB service.finish growth before sampling.
