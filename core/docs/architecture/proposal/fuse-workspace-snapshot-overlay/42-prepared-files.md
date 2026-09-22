# Prepared fresh regular files

> **Status: implemented and functionally verified for the declared prepared-file scope.**
> Implementation parent: `82c87d70a1f3e723b627bf1d0777305c9eb393ea`.
> Exact implementation commit: `ab473145a606a71327d55d10f12205edb1803946`; [confirmation](evidence/prepared-files/commit-confirmed.json).
> Product input seal: `3c40e8466e8dfefeeb8f6f85e51d6cd23fe9eab42ebac8e940e53c6ae06c5fc3`.

This round extends the existing direct prepared filesystem request and C5 prepared
changes with fresh regular-file declarations. File content and portable metadata
remain separately saved prerequisites; this adds no Workspace create operation,
implicit Commit, allocator or canonical builder in Workspace. The existing C1
filesystem update owns namespace reference counts and the final root.

## Shared admission and wire compatibility

`new_file_serials` (F) is a sorted, unique subset of the existing inode rows (I).
Each ID is positive and in the existing signed63-bit identity range, is not the
root serial, and matches exactly one kind1 row. Existing disjointness between I,
new directories (N) and existing-directory patches (P) also excludes F/N/P overlap.
The combined inode bound remains I+N+P<=128; F does not count the same file twice.
The complete request metadata ceiling remains32768 bytes.

| Declared extension | Encoding | Extension bytes |
| --- | --- | ---: |
| N=P=F=0 | Existing omitted trailer | 0 |
| F=0 and N+P>0 | Existing v1, unchanged bytes | 5+24(N+P) |
| F>0 | v2, N/P records followed by u16 F count and u64 IDs | 7+24(N+P)+8F |

A v2 trailer with empty F is rejected. Decode bounds precede allocation: N and P
share the remaining128-I capacity and F is at most I. Extracting the prepared
codec into a focused implementation file preserves responsibility and the999-line
ceiling; relocation is not an algorithmic source reduction.

## Service and C1 ownership

An inode row absent from the selected base must be declared fresh, and a declared
fresh row must be absent. Existing rows retain their old kind/reference count and
existing-directory content constraints. Fresh regular-file values enter C1 with
reference count zero, alongside one sorted union of N and F in `new_inodes`.
C1 derives the retained reference count from actual name bindings.

Direct fresh-file rows use the existing `validate_inode_role` helper to validate
actual File content and kind1 portable metadata, not merely authenticated object
presence. C5 retains its existing validation of every inode row. Direct legacy
existing-row semantics remain unchanged. Existing save/deadline/abort and retained
failure ownership remain in place; already-finished content/metadata prerequisite
saves are not deleted because a later filesystem update fails.

Unbound fresh regular files are rejected by C1 as `new inode without binding`.
The omission rule for unreachable new directory parents does not apply to them.
This distinction is established by existing `filesystem_hardlinks` and
`filesystem_topology` tests and the reference reducer. Base absence is not allocator
provenance: callers still reserve new identities through the live allocation scope,
and no new guarantee is inferred from the declaration alone.

Workspace supplies an empty F list at this checkpoint, so existing file/directory
capture and exact v1/omitted request accounting are unchanged. Actual fresh-file
state, capture/lowering and successor reconciliation belong to native create.

## Actual proof and remaining work

Two native selectors passed: successful fresh-file use across direct save,
Stage/CommitStaged and composite Commit; and malformed declaration, role, base
identity and unbound-file refusals. Each successful path uses distinct live
reservations, pre-saved zero/nonzero file content and portable metadata. Existing
closed masters are byte-copied; C5 authority is fresh. The successful selection
reserves six IDs and uses separate pairs for direct save, Stage/CommitStaged and
composite Commit. It checks exact tiny/empty contents and attributes, hard-link
reference count2, unchanged old roots, and exactly two explicitly requested
workload Commits. A following reservation proves no additional range was consumed.

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| files-01 | PASS | 3.042074959 | 3.148357041 |
| refusals-01 | PASS | 1.984490583 | 2.100792000 |

These are first functional samples, not cold/performance measurements. All nine
selected checks pass. The refusal selection records54 attempts:18 variants over
direct, Stage and composite routes, with local NOT_SUBMITTED shape refusals
distinguished from real Service failure terminals. Every refusal preserves the
prior acknowledged Stage, Branch and history; the Stage is then explicitly
discarded. Both selections close their host client/Service normally and verify
the closed master is unchanged. Each complete command has a60-second budget and
one construction worker. They use host-native runtime, so Docker CPU/RSS claims
are not inferred. No passing sample was rerun and no fixture was regenerated.

The [functional index](evidence/prepared-files/functional-index.json.gz),
[check index](evidence/prepared-files/checks-index.json.gz) and
[archive manifest](evidence/prepared-files/archive-manifest.json) retain commands,
receipts, exact sources/binaries/callers/imported helpers, ownership journals and
all client/Service logs. Independent product and caller reviews found no required
correction. Locked/offline Rust1.85.1 checks pass: host **691 tests/3 ignored**,
Linux **689 tests/136 ignored**, both all-target Clippy commands with warnings
denied, host binary/example build, fmt, the252-file boundary guard and six guard
tests. The new cases add five external Rust tests. Docker verification uses the
pinned tool image, two CPUs and Cargo jobs2. CI and retired preflight did not run.

## Resources and source size

[Linux compiler layout evidence](evidence/prepared-files/layout/layout-01.json.gz)
records PreparedChanges296->320, Operation304->328 and Request344->368 bytes;
Response remains112 bytes. F adds one24-byte Vec header and at most128*8=1024
bytes of requested element capacity. Service borrows F; its existing new-inode
scratch remains N+F<=128 IDs/1024 bytes. Serial lookup still requires at most384
IDs because F is already a subset of I, and final values remain bounded by128.
Role validation processes one fresh row at a time through the existing FileView
and portable reader. These are source/capacity and compiler-layout observations,
not allocator/RSS/cgroup measurements. No new persistent state, worker, save or
changed resource policy was introduced.

Production LOC: **111922 -> 112031 (delta +109)**; core **46505 -> 46614 (+109)**,
reference **65417 -> 65417 (+0)**. Method: exact first parent and final staged
product snapshots with unchanged `tools/production_loc.py`, blob
`b5b9617d08204977176302311e0b2c72a811b420`; count nonblank/noncomment product
Rust/runtime SQL, excluding tests/docs/tools/manifests/generated output.
[Per-file/folder/crate inventory](evidence/prepared-files/source-loc.json) retains
the complete scope. Existing prepared codec helpers relocated into prepared.rs;
that movement is not an algorithmic simplification or legacy retirement.

The full preinstalled DSH tree remains unchanged and must ultimately be uploaded
in full before one explicit Commit. Native/mounted file creation, symlinks, larger
input admission, repeated incremental Commits, hard memory qualification and
matched R6/#207 remain open.
