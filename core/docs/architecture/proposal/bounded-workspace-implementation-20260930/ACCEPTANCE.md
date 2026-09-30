# Integration, failure and evidence specification

> **Status: Research; informative and not a product contract.**
> Implementation specification proposal, 2026-09-30. Source baseline:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. This packet specifies future work;
> no product implementation, test, benchmark, merge or release is claimed.

Read the [integrated specification](README.md),
[Workspace packet](WORKSPACE.md), [Server packet](SERVER.md),
[concurrency packet](CONCURRENCY.md) and [scenario catalog](../../../../../scenarios.md).
The component contracts and repository rules remain authoritative. Source-backed
facts, design choices, source-derived arithmetic and measured receipts are
different evidence classes throughout this packet.

## 1. Joint invariants

1. SDK workload proofs enter through public SDK lifecycle and opaque
   `WorkspaceApi::exec`, the real mount, ordinary filesystem operations and the
   same production live/Commit path. Authorized externally launched processes
   use the same mount/actor/issued-handle mutation authority, with their own
   route label and explicit external supervision; they do not replace SDK
   coverage. Product code never recognizes a command, executable, benchmark,
   file extension or model/database/package workload.
2. An accepted syscall publishes one authorized private revision. Capture
   assigns that publication to G1 or G2. A whole Exec is not a transaction;
   cancel, nonzero exit and disconnect preserve already accepted changes.
3. Immutable G1 remains readable while G2 changes. Known-result installation
   preserves the current G2 and exact selected old readers, including inserts,
   truncation, rename, aliases, fresh/open-unlinked files and orphan versions.
4. Logical capture/install selection is bounded independently of changed
   populations. Count actual root-catalog path I/O and lock residence separately;
   a constant number of selected roots is not zero page work.
5. Every live population has paged exact authority or an admitted finite owner
   envelope. Account allocator capacities, caches, registry entries, output,
   sockets, stack memory, SQLite heap/journal and OS file cache in their domains.
6. READY owns all predictable install/failure resources before Branch send.
   G2, other Workspaces and command admission cannot consume protected progress
   credits. Unknown outcomes preserve exact owners; no guessed replay/refund.
7. One pending Commit/Stage per Workspace, one construction producer per
   admitted Commit/Stage operation, unchanged configured Store capacity, explicit shared
   Branch conflicts. No daemon-wide logical Commit exclusion or extra helper
   producer is introduced to earn a performance result. Keep the repository's
   explicit namespace-init exception.
8. Large candidate construction holds only its declared conflict scope.
   Same-inode mutations may serialize; unrelated file writers/readers must
   progress within available capacity. No registry/publication lock spans a
   shell, bulk transfer, graph walk, sort, canonical construction or teardown.
9. Disk-backed state remains quota owned until actual release. Pins, failed
   owners and unfinished cleanup retain charges. Moving a buffer or root does
   not return its accounting credits.
10. Platform adapters supply explicit capabilities. No APFS-specific behavior,
    child-memory illusion from output bounds, silent unsupported fallback or
    added sync/WAL/crash-durability promise enters the logical architecture.

## 2. Failure counterexamples and architecture obligations

The owner selects a replacement architecture, rather than patching the current
paths until the listed examples pass. The findings below motivate enforcing
boundaries in the new architecture. They are not a fix-first implementation
queue. Sound canonical/storage components can remain; old representation and
ownership hazards must not remain as an automatic fallback path.

| Packet | Class and current evidence | Required design/result | Scope boundary |
| --- | --- | --- | --- |
| W-Live / W-Commit | Scalability and allocation safety; current complete affected-range, descriptor and reconciliation populations are source confirmed | Stream private candidates, final-state lowering and generation installation; page owner/retirement authority | Raising a quota or count does not satisfy bounded memory |
| S-Input / S-Graph / S-Draft | Scalability; prepared bindings, filesystem graphs and unfinished drafts can be resident populations | Incremental checked parser, ordered reductions and exact external proof/drafts | Complete graph proof can remain namespace wide |
| C-Registry / C-Exec | Features and operational correctness; singleton control/mount/gates and finite Exec lifetime exist | Multiple identities/mounts, per-Workspace submission, admitted channels and exact command domains | #249 enablement waits #248 prerequisites |
| B-Cache | Narrow source finding: edit load path inserts without the caller-required cache room step | Enforce room/charge before admission and define refusal custody | Prospective reproduction required; no measured peak claimed |
| B-Prepared | Source finding: directory parser reserves whole binding vector before final refusal | Check bytes/counts before allocation and stream individual bindings | Check malformed/truncated input without hidden buffering |
| B-ReadFrame | Source finding: public pinned-read size and native terminal framing/length field disagree | One checked advertised/encoded limit or explicitly negotiated versioned streaming reply | Small successful reads do not qualify 32/128 KiB |
| O-Deep / O-Move | Optimization; deep270 cost retained in #276, path-local pure-move scope remains unproved | Exact single graph proof first; authenticated owner index is a separately compatible extension | No weakened final-batch cycle/alias checking |
| O-Hot | Optimization; normalizing/retired pages require exact reachability and fence proof | Work-triggered bounded maintenance and safe reclamation | No ancestor deletion from a timing hypothesis |
| B-Statfs | Current adapter reports zero total/free blocks | Truthful quota snapshot projected through ordinary statfs | No unrelated mount/provider policy change |
| B-Interrupt / B-Death | Provider/runtime gaps under #259 | Explicit bounded callback/interrupt and dead-mount recovery contract | No third-party patch; no daemon-crash data recovery claim |
| D-Discard | Missing explicit dirty-discard workflow; historical exit7/unmount assumption fails | Explicit selected private-discard operation if enabled; retain current dirty-unmount guard | A failed command's accepted writes are not automatically erased |
| E-Memory | Host measurement capability under #283 | Correct phase observer with independently checked descriptor and domain semantics | Does not qualify cache equality or change product code |

Issue #276's older body also describes absent public pins, unmerged integration
and test reds from old branches. Use the current
[ticket audit](../../../issues/286/TICKET-AUDIT-20260930.md) and
[seven-family checkpoint](../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
before assigning current failures. Historical reports/receipts are immutable;
an old red is not a current-source reproduction.

## 3. Projection and workflow contracts in the target architecture

### B-Statfs: shared quota and protected capacity

Current [Adapter::statfs](../../../../crates/layerfs-fuse/src/adapter.rs)
returns zero totals/free counts. Add a small semantic Workspace quota snapshot;
FUSE formats it in a dedicated quota projection file. The adapter never scans
owners or Store tables to answer statfs.

For block unit p=4096, let effective total be the smaller declared Workspace
ceiling and its applicable shared pool ceiling. Available bytes are the minimum
of that Workspace's remaining admission and shared allocatable capacity after
actual allocations, outstanding reservations and protected completion shares.
Round down when converting to free blocks. Do not subtract a reservation twice
after it transfers to allocated ownership, and do not advertise another
Workspace's protected fund. A snapshot can become stale after its short read;
subsequent writes still perform authoritative admission.

File-count fields use an explicit declared identity policy or documented unknown
values; invented unlimited inode counts are not a quota guarantee. This remains
ordinary statfs for every program. The unmerged statfs branch is a source donor,
not automatic acceptance or authority to overwrite current mount settings.

Cover empty, allocated, reserved, pin-retained and cleanup-retained cases, then
two Workspaces sharing a global pool. Check df/statvfs against the semantic
snapshot and ordinary writes reaching/refusing declared capacity.

### Runtime actor and mount admission

The contained-execution architecture needs an actor identity independent of the
privileged serving daemon and canonical portable mode/mtime. Current Adapter
guard admits only Workspace root uid; mount capability validation additionally
requires root uid0 and SessionACL::Owner. A child credential drop therefore
cannot be appended to the old mount profile and assumed functional.

The target runtime profile explicitly binds Workspace/incarnation to an admitted
numeric actor UID/GID and projected ownership policy. Domain-control/private
backing remains supervisor owned and inaccessible to the actor. The mount provider
must admit that actor to the session, then the semantic projection must reject
unrelated actor identities before processing operations. Report projected uid/gid
consistently with kernel DefaultPermissions and mode checks. Retained command or
mount ownership keeps the actor assignment; reuse only after exact cleanup.

Select the supported ACL/privilege mechanism in CONCURRENCY's runtime profile,
not a global allow_other change hidden in statfs or a blanket request-guard removal.
Prove real ordinary open/read/write/rename/exec at the actor credentials, proper
mode refusals, root/callback identity and sibling actor denial. Strict filesystem
confinement additionally hides outside paths/FDs and remains an explicit capability.
Unsupported required actor/domain/mount composition fails before user code runs.

### B-Interrupt and B-Death: provider scope

Keep the current 128 KiB request envelope, direct I/O, TTL zero and
max_background=1 unless a separate prospective resource/capability design changes
them. Concurrent Exec does not require increasing these controls. Mount/session
resources are charged per admitted Workspace; they are not per-command helpers.

The pinned fuser interface does not deliver a cancellation callback to Workspace.
Its bounded-step observation remains a legacy capability description; it is not
the end-state interrupt solution. Select an owned Linux kernel-session adapter
for the target cancellable projection, implemented from the kernel UAPI rather
than a vendored/patched provider. Reuse semantic Workspace projection/argument
checks. This is a platform I/O boundary, not a new filesystem algorithm or a
special command route. Unsupported required session capability fails at mount
admission; no silent legacy interrupt fallback.

The kernel reader owns a byte-admitted request window and prioritizes INTERRUPT
delivery independently of the callback's blocking work. RequestLease binds
mount/incarnation, kernel unique ID, actor or issued-handle authority, finite
callback deadline, cancellation state, publication receipt and single-use reply.
Use a keyed live request index; completed history is not retained. Unknown-target
interrupts have bounded pending/control state and the protocol's EAGAIN outcome.
Before private publication, cancellation wins the short publication race and
the candidate remains unselected/owned for cleanup. After accepted publication,
finish the actual result and retain cleanup; no EINTR fiction or rollback.
Check cancellation between bounded preparation/I/O steps and at publication.
This provides cooperative bounded-step cancellation, not preemption of arbitrary
kernel/device waits. Cmd cancellation and syscall cancellation remain distinct.

The [kernel FUSE interrupt contract](https://docs.kernel.org/filesystems/fuse/fuse.html#interrupting-filesystem-operations)
permits interruption before/after original delivery and reply races. Implement
its matching and response rules explicitly. A candidate can't become visible
after the adapter has answered its original request as cancelled-before-publication.
The reader/dispatcher do not add a constructor helper, increase max_background
or hold the namespace publisher during a callback. Reuse existing admitted mount
thread resources or an explicitly accounted fixed readiness worker set.

Target module responsibility: kernel wire decode/encode; live request/cancel
ownership; readiness/session delivery; mount access/detach authority; ordinary
semantic dispatch. Each is a named <=999-line production file, entry modules
<=200 declarations/delegation. Hand-write checked field access from documented
ABI; no copied third-party crate source or test-only dispatch shortcuts.

```text
layerfs-fuse/src/
  lib.rs                        <=120  public mount/semantic delegation
  projection/
    identity.rs                 <=350  RuntimeActor/issued-handle authority
    files.rs                    <=650  ordinary open/read/write/size projection
    names.rs                    <=650  create/link/rename/unlink projection
    listing.rs                  <=500  issued directory/cookie paging
    attributes.rs               <=400  modes/mtime/errno/kernel attributes
    quota.rs                    <=300  truthful shared quota snapshot
  linux/
    wire/
      requests.rs               <=650  checked kernel UAPI input fields
      replies.rs                <=650  bounded single-use reply encoding
    requests/
      leases.rs                 <=450  live unique-ID/actor/reply ownership
      interrupt.rs              <=400  cancellation matching/publication race
    session.rs                  <=650  reader/readiness/admitted dispatch
    mount.rs                    <=600  exact mount/capability/detach custody
```

Reuse existing semantic argument/permission/reply helpers by responsibility.
This is a target file map, not empty scaffolding or a claim each file needs its
maximum. Module entry files are declarations/delegation only. External real
kernel tests remain under tests; source imports never include test fixtures.

For orderly teardown, close new admissions and stop/reap the selected execution
domain, drain exact accepted callback/handle/submission owners, detach the mount
once, confirm absence, then release its count slot. A failed detach or cleanup
retains ownership. Independent registry entries do not take that long wait.

For daemon death, the runtime supervisor owns a precise stale mount lease/path
and a bounded explicit detach/recovery action. AutoUnmount is selected only
after the provider and privileges prove the required behavior. Never infer
canonical outcomes from a dead daemon or delete retained unknown owners to
reclaim the mount path. Crash data recovery and durable Commit remain outside
this architecture packet.

Future external tests signal callers during read/write/rename, record syscall
returns and final published state, and kill only an owned serving daemon while
checking stat/ls/mount absence and subsequent attach. No private test hooks or
product fault-injection branches are introduced.

### D-Discard: explicit semantics

Keep unmount as mount detachment, not dirty-state discard. A separate future
DiscardPrivate capability must name the exact Workspace/incarnation and live
selection. Acquire a scoped workflow barrier, finish or explicitly cancel owned
commands, and drain conflicting callbacks. Reject a retained/Unknown submission;
never guess the Branch result. Change only the private live selection to the
declared known baseline. Existing selected views keep their old immutable state
and charges until release; bounded retirement owns abandoned private pages.

Stage discard remains C5 metadata semantics and does not imply C2 content GC or
reuse/refund of consumed inode serials. Specify wire authorization/outcomes
before enabling DiscardPrivate. The historical exit7-then-delete FAIL stays FAIL;
tests exercise explicit discard separately rather than removing dirty guards.

## 4. Scenario-to-proof map

Every selected row freezes exact identities, command, oracle, cache/resource
profile and phase scope before execution. The named larger tiers are design
pressures, not authorization to run an automatic scale sweep.

| Scenario | Core future evidence | Key adversarial variant |
| --- | --- | --- |
| SC-01 rapid mutations | Repeated versus separated writes, final surviving runs, charged live/Commit populations and exact successive roots | One million WRITE calls can leave one run; count final state separately |
| SC-02 large inherited file | Sparse edit bytes/boundary reads/page visits; fragmented wide overwrite and shrink; selected old bytes | Plan crosses capture/install while same inode remains protected; holes/EOF and shifted source coordinates |
| SC-03 many tiny files | Many-file and single-wide-directory cursors, exact graph/aliases, inode reservation and owner ledger bounds | Larger-than-TinyPack files, allocator exhaustion, listing after rename and pins |
| SC-04 bulk files/logs | Real acquisition/transfer bytes, bounded heap plus file-cache residency, actual temporary high water | Fresh/full replacement pays all bytes; ordinary prepend/copy is not an optimized splice |
| SC-05 namespace mutations | Atomic final-batch names/parents, cycle/alias refusals, old/new subtree bytes | Destination replacement, directory move, open-unlinked handles and multi-key failure |
| SC-06 generation overlap | Accepted writes during every real Commit phase, current G2 survives install, next parent/root and terminal source depth | Known remote/local failure; capture while a detached mutation candidate is prepared |
| SC-07 custody/output/admission | Exact refusal/Unknown/known/install/cleanup outcomes, pins and physical credits, noisy output drain | Exit7 accepted edits, response loss, final-pin release and protected completion under G2 pressure |
| SC-08 concurrency | Independent ordinary SDK Exec, 1/2/3 mounts, overlapping admitted Commits and true command domains | Same Branch HeadMoved, quiet lifetime, cancel/disconnect, setsid descendant, no Store permits for fresh IDs |

Current smaller profiles establish compatibility before larger file/body/count
profiles are changed. Named ceilings receive separate arithmetic/format/resource
review. Unknown/owner-deferred 10240 is not silently activated by this plan.

The target namespace/file-construction profiles have their own independent
expected-root ledger. V1 history/read compatibility and new v2 migration/locality
are distinct proofs. The schema11 maintenance proof verifies all configured
Store users drained/closed, indexed locator copy and ordered equality, bounded
schema swap and per-batch journal/heap, precise Unknown/cleanup custody and
preserved payload packs/save IDs/ordinals. Source-only process-local arbitration
does not prove cross-process exclusion. Record metadata peak and final allocated
freelist separately; do not claim physical refund merely from DELETE/DROP.

## 5. Evidence sequence and reuse

1. Review the typed stream, generation, READY, resource, command-domain and wire
   contracts together. Match all cross-packet ownership; decide compatibility
   before a producer/consumer changes.
2. Implement a coherent packet at a sealed source. Reuse existing public API
   fixtures, existing exact roots and identity-matched unaffected proofs. Do not
   use direct Workspace/Service writes as evidence for the ordinary SDK route.
3. Diagnose narrow changed behavior from external public-API tests, allocation
   paths and prior receipts. No tests inside product src, test-only counters,
   retry hooks or alternative benchmark code paths.
4. Select only affected performance mechanisms prospectively. One sample per
   case/arm, independent verification separately, clone prepared input outside
   timers, no warm credit, no raised deadlines/workers/quotas to turn a miss PASS.
5. Report end-to-end, Exec, Commit, capture/install wait, Server construction,
   actual cleanup, resident domains and temporary/final allocated bytes. All
   registered FAIL/INELIGIBLE/INCOMPLETE/NOT_RUN rows remain visible.
6. At final changed source, run owning locked Core checks and boundary guard
   once as required. No CI/preflight aggregate. Exact per-commit production LOC
   compares first parent to final staged/committed tree with unchanged method.

Family2 history proofs are reused while their canonical/codec/C2/C5 measured
mechanisms are unchanged. If C2 authority/index/provider work changes them,
identify the smallest necessary regression checkpoint explicitly; a broad
documentation/harness seal alone does not trigger another full history group.
The accepted up-to10% allocated-history tolerance remains; it does not define a
temporary-disk allowance. No roughly50% speed loss for roughly5% storage saving.

## 6. Phase memory observer correction to investigate

The authoritative [Linux6.12 cgroup-v2 specification](https://docs.kernel.org/6.12/admin-guide/cgroup-v2.html#memory)
defines memory.peak reset and subsequent observation for the same open file
descriptor. Reading a newly opened descriptor can report the lifetime domain.
The historical #283 reports record successful writes and unchanged readback but
do not establish descriptor custody in the cited summary. This is a specific
method hypothesis, not proof of a Docker/kernel defect or a repaired observer.

The next capability proof must retain one O_RDWR descriptor from reset through
all phase reads, seek/read it as required, verify the exact owned cgroup, and
demonstrate a controlled new peak after release/reset. Record a separate new
descriptor's lifetime reading distinctly. Check the deployed kernel behavior,
not only a write return code. No probe was run for this specification.

A correct peak observer still does not establish cold/cache equivalence, child
versus LayerFS attribution, or a bounded resident-window provider. Instrument
anon/file/shmem/kernel/socket contributions and overlapping phases explicitly.
Keep old numeric receipts INELIGIBLE and existing controls NOT_RUN; never promote
their numbers after improving the observer.

## 7. Readiness for implementation and completion

The specification is ready for implementation when cross-packet interfaces and
compatibility gates are explicit, all live populations have an authority/owner,
and the source/custody/output/process capability envelopes are reviewable.
Implementation completion requires the covering evidence above at exact source
identities. A document, structural bound or finite successful family does not
establish arbitrary scale, independent throughput or memory qualification.
