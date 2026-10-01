# Phase 6 real SDK/FUSE/storage integration experiment

> Status: Research; informative and not a product contract.

Owner instruction: #294 must exercise actual commands, Linux FUSE, daemon SQLite,
canonical construction, acknowledged MinIO objects and conditional global history
publication. Parent source: `ea369fe39a6e30a5be215591f5f7cad446d1abbc`.

## Frozen first delivery

An isolated research runtime under `core/benchmark/phase6-live/` uses the existing
public `layerfs_sdk::WorkspaceApi`, `layerfs_sandbox::SandboxOwner`, authenticated
Bridge control connection and operation codec. A separately built experimental
daemon supplies a real `fuser::Filesystem`; the shipping FUSE adapter is coupled
to the old Workspace and cannot substitute the SQL prototype. No product sources
or third-party sources are changed. An arbitrary command is passed unchanged to
`/bin/sh -c` with the FUSE mount as its working directory.

The first complete correctness case creates a 4 KiB file, reads it through FUSE,
commits, overwrites it using an ordinary command, commits a successor, and checks
both published versions from MinIO after the daemon has been removed. The
independent expected bytes come from the literal command inputs and offsets;
namespace and portable metadata expectations come from an independently recorded
manifest. C1 authenticated readback proves stored identities, separately from
the independent semantic oracle. Independent canonical root vectors and the six
larger cohort selections remain subsequent gates; candidate roots are never used
as independently expected roots.

## Authority and ordering

```text
host: real WorkspaceApi / SandboxOwner
       | existing authenticated Bridge control
Linux experimental daemon: shell -> kernel FUSE -> SQLite live metadata
       | one construction producer, actual C1 streaming file/tree construction
       | actual C2 encoding/pack assembly -> MinIO PUT -> HTTP acknowledgement
       | authenticated experimental metadata protocol
host metadata authority: ready object locators -> actual C5 stage/commit
       | short SQLite transaction, conditional expected Branch/head/base/root
daemon: known result -> selected root installation -> explicit owner drain
```

Global object locators and C5 history use separate databases owned by one host
process. Payloads exist in MinIO, not in global SQLite BLOB storage. Locators are
inserted only after a successful object PUT. The service authenticates the uploaded
pack and candidate before publication. A failed or uncertain publication retains
the pending candidate; it is never resent or inferred from a head query. There is
no cross-database transaction or added crash-durability claim.

The separate experimental metadata protocol uses the existing authenticated
native connection and bounded Begin/Success/Failure frames. Its versioned payload
namespace is `P6META3`; it does not allocate or reinterpret product operation tags.
The payload starts with the seven ASCII bytes `P6META3`, followed by an action
byte. Integers are big endian; identities keep their actual C1/C5 widths.
Action 0 returns bootstrap configuration (bounded u16-length UTF-8 fields), the
fixed experimental Branch snapshot and one consumed inode reservation. Action 1
accepts a 32-byte object ID and returns presence, role, canonical length and the
32-byte pack digest. Action 2 accepts that same locator tuple, authenticates the
acknowledged object and registers it, returning an empty success. Action 3 returns
the current fixed Branch snapshot (stack 17, branch 17, base 33, optional head
1+33, root/scope/profile 32 each). Action 4 accepts Workspace incarnation 32,
generation u64, captured snapshot and candidate root 32; its successful response
is the existing encoded `WorkspaceCommit` response with a real C5 outcome.
Serials are reserved once during bootstrap, never reused; the first admitted
512-inode profile consumes a fixed reservation of 512 serials. Every decoder
requires exact EOF, at most 16 KiB metadata, and one matching request ID. One
request/response pair owns its connection. Unsupported versions/actions fail.

## Explicit experimental profile

The daemon is Linux arm64 in an owned Docker container with `/dev/fuse`; host
MinIO and the metadata authority run on macOS. This is the requested Phase 6
topology, distinct from existing product benchmark topology. SQLite is the locked
rusqlite 0.40.2 provider, using its existing bundled feature for cross compilation;
the bundled header reports SQLite 3.53.2, which must be checked at runtime. MEMORY
journal, synchronous OFF, temp MEMORY, busy timeout zero; no WAL or new fsync.
The root ARMv8 AEAD flags are build inputs. No foreign Cargo target is used.

The first path serializes Commit against mutations. It has no concurrent G2
capture/install claim. SQL owns final bindings and final interval extents; payload
write sources are immutable. File construction streams those extents and emits
real C1 objects. Changed files may be reconstructed in full; this does not pass
the localized one-edit gate. Unchanged file roots are reused.

The initial metadata construction profile admits at most 512 live inodes and 512
bindings in one directory before construction; it does not claim indefinitely
streamed namespace construction. SQLite cache is explicitly bounded. A write
source and each object/pack use existing finite format bounds. Handles have an
explicit bound and are retired on release. Unsupported syscall capabilities
return an error. Large profiles, locks, mmap, permissions beyond the declared
single owner, parallel commands, failure recovery and mount-death proofs remain
unqualified.

For the first end-to-end path, payloads use C2 FULL/compression and pack assembly.
Canonical inode leaves use an explicitly experimental ordinary FULL pack
representation with their true role/identity, decoded by the C2 ordinary decoder.
This is a versioned experimental physical profile, not the shipping pooled
metadata profile. Pooled metadata, delta selection and dense multi-object packing
remain separate integration gates and no storage/speed equivalence is claimed.

## Prospective evidence and completion gates

First correctness invocation: one fresh owned run directory and isolated MinIO
bucket, one construction producer, exact source/binary/image/workload identities,
actual SQLite version/settings, FUSE callback counts and SQL extent/source counts,
acknowledged object/pack bytes and publication head records. Full execution,
construction, upload, publication and cleanup are observed; no phase is skipped.
No speed admission is made: source/SQLite/MinIO OS-cache state is unknown and
reported INELIGIBLE. No unchanged arm is repeated to obtain a better time.

Before speed collection, publish a separate case/cache/limit specification and
seal the coherent implementation. Complete commands retain the 15 s budget;
separate proof has a 9.5 s bound. A build or provider/correctness diagnostic is
labelled as such and never promoted into a performance row. Physical resource
and healthy-progress proofs remain unrun until supported observations exist.

- [ ] Real MinIO/C2 provider and authenticated readback.
- [ ] Real C5 conditional publication and retained uncertain custody.
- [ ] Real SQL-backed FUSE mounted by the existing public SDK.
- [ ] First complete create/overwrite/two-publication case and explicit cleanup.
- [ ] Independent semantic readback after daemon removal.
- [ ] Localized edit construction; shipping pooling/delta/packing composition.
- [ ] Six cohort commands, canonical reference vectors and resource proofs.

The next action is implementation of the MinIO object consumer/read provider and
the single-owner metadata authority, then composition with the SQL FUSE daemon.
This document is a prospective experiment specification, not an implementation
receipt or a completed milestone.
