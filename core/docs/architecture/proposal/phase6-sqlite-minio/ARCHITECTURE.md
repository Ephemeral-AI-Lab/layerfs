# Phase 6 simplified ownership model

> **Owner storage-placement amendment, 2026-10-02:** The strict
> [SP1 specification](implementation/sp1/IMPLEMENTATION.md) supersedes earlier
> physical placement descriptions here. Global SQLite holds all committed
> filesystem metadata and pooled values/groups. MinIO holds regular-file
> whole-payload and CDC chunk packs only. Canonical identities and historical
> evidence remain unchanged; the strict replacement is prospective/unverified.

> Status: Current planning checklist; no release candidate exists.
> Owner-selected revision,2026-10-02. Owning issue[#293](https://github.com/Ephemeral-AI-Lab/layerfs/issues/293).

## Selected responsibility split

The owner authorizes implementation of daemon-owned live SQLite metadata and
filesystem correctness/construction, direct immutable MinIO uploads, and a small
global locator/identity/history/conditional-publication API. Remove duplicated
service-side candidate/portable/file certification and namespace installation.
Avoid quadratic work. Historical experiment receipts keep their original source
and verdict; this decision does not promote them to the revised source.

```
Before: command -> FUSE -> daemon SQL/C1/C2 -> MinIO
                                |
                                +-> Authority -> pack/root/attribute MinIO GETs
                                         |       namespace/file/tree proofs
                                         |       second namespace index/install
                                         +-> global SQL locators/history/Branch

After:  command -> FUSE -> trusted daemon SQL/C1/C2 -> MinIO
                                |
                                +-> small API -> global SQL locators/history/Branch
```

|State|Owner|
|---|---|
|Writable names/inodes/attributes/extents/handles/dirty state|Daemon SQLite|
|Captured generation and complete canonical construction|Daemon|
|CDC/hash/compression/packing and acknowledged uploads|Daemon + immutable MinIO|
|Committed filesystem mappings|Existing immutable canonical trees selected by root|
|Object-to-pack locator selection|Global SQLite through its owning API|
|Allocation/identity,Commit/Layer history,conditional Branch head|Global SQLite|
|Independent semantic/canonical/resource qualification|Separate owning proofs|

Global SQLite selects committed roots; it does not maintain a second authoritative
copy of the complete filesystem. Existing inode-based bindings preserve descendant
identities on directory rename and avoid full-path rewrites. Keep actual C1/C2/v1
compatibility; the experiment's FULL-only packing does not prove shipped delta/
pooling compatibility. Delta-base reachability and bounded reconstruction remain
required when those formats are enabled.

## Trusted publisher and storage consistency

Authenticated first-party daemon publication asserts that captured construction
and upload/locator completion are finished. The normal service checks request
identity/framing/EOF,scope/profile/owner/generation,registered root role and exact
expected Branch selection; it performs C5conditional publication without fetching
candidate packs. No automatic retry,fallback,guessed adoption or Unknown refund.

Arbitrary commands must not access daemon publication secrets or private backing
metadata. First supported Linux experiment uses a root supervisor and a separate
nonroot command identity,clean environment,no-new-privileges,cleared capabilities,
non-dumpable supervisor and root-only backing directory. Actual provider tests must
prove this boundary before trusting the new path. Container administrator/host
root are trusted; stronger confinement,multi-tenant credentials and cloud deployment
remain separate capabilities. Generic commands still use the real FUSE mount.

Seal physical packs by their digest and use actual conditional create/immutable
collision checks. Canonical object identity remains separate from pack identity.
The daemon authenticates existing dedup objects and reads; service locator rows
accept an authenticated completed upload assertion and enforce immutable role/
length/selection. Concurrent valid alternate packs may exist; retained locator
selection never changes on a registration race. No per-Commit whole-provider scan.

Publish after uploads and bounded locator registrations complete. Losing/refused
candidates may leave immutable packs. Global Commit/Branch publication is atomic in
C5; local locator/history databases need no joint transaction because publication
references already registered immutable objects. Preserve unresolved/accepted
ownership and pins; safe GC/import/restart are independently qualified capabilities.
Multiple hosts use a small API to one global SQLite owner,never a shared database
file. MEMORYjournal/OFFsync experiment is not cloud durability or replication.

## Priority and retained evidence

[Retained V4c2b2 report](implementation/RESULTS-V4C2B2.md):128file mutation
628.198625ms,publication532.306917ms,authority927GETs/6369311B;270initial
Commit1284.607125ms,publication1112.926709ms,1937GETs/14218069B. CacheINELIGIBLE;
GET intervals cover registration+publication,not individually timed certification.
No exact speed result is predicted by subtraction. The new normal authority path
should perform zero MinIOGETs; only the independent verifier reads committed data.

1. Simplify trusted ownership and normal publication; prove128file mutation.
2. Prove270component initial Commit and shallow localized successors.
3. Finish admitted populations/inherited import,generic syscalls and remaining
   owning families/fullDeepSeek/locality/resource/Unknown/compatibility gates.

270component successor Exec latency optimization is deferred nice-to-have by owner;
its existing deepest-file correctness evidence is preserved. No TTL/deadline/worker
increase. Shallow arbitrary edits and no quadratic scaling remain required.

Concrete next delivery:[S1-SPEC](implementation/S1-SPEC.md). Older V4c3c1reservation
slice preserved PARTIAL atc92263bfbb93a2f7bd0cf7a68479ed184dadf127;passed checks,
live gate unrun. Current owner direction replaces that old next action,not history.
