# S8 prospective proof plan

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-08 on local `main` at `32d969776` (product source pin
> `f0797c646`). This is a prospective plan. It registers nothing, freezes no
> benchmark selection and contains no new measurement result. No build, test,
> mount or measurement was run to write it. The
> [S8 specification](S8-SPECIFICATION-20261008.md) owns every decision referenced
> here.

## 1. Purpose and rules

The plan defines how a future S8 implementation must prove function, counts,
resources and, last and separately, speed with storage. Order is fixed:
deterministic function first, falsifiable counts second, resources third,
timing only after those at a frozen identity.

Rules that bind every row, from the [root guide](../../../../AGENTS.md), the
[measurement workflow](../../../../docs/general/agent-measurement-policy.md),
the [benchmark rules](../../../../docs/general/benchmark_rules.md), the
[report template](../../../../benchmark_agent_report.md) and the
[core harness guide](../../../benchmark/fs-bench-pro/AGENTS.md):

- Disposable/WAL/OFF selected explicitly; Durable is
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- One sample per selected case and arm; no best-of, no unchanged resample, no
  timeout, workload or cache change to turn a miss into a pass.
- Fresh append-only outputs; every failed, ineligible and unrun selection is
  kept.
- Each measured phase pays for its own work from an equally declared and
  enforced cache state. Unknown or mismatched state is `INCOMPLETE` or
  `INELIGIBLE`.
- `LAYERFS_CONSTRUCTION_WORKERS=1`; FUSE dispatch concurrency is a separate,
  declared setting.
- Existing budgets: complete performance command at most 15 s by default,
  declared exceptions up to 25 s, independent proof under 10 s, frozen family
  limits only in their own scope, test commands at most 120 s. Conflicts are
  listed in section 8 as pending owner decisions; none is resolved here.
- A sampled or scoped content proof is called sampled or scoped.
- Fault-holding fixtures live outside product source.

## 2. Historical evidence and eligibility

Originals are on experiment commit `1451b68a720bbe2175a103dd9b35693ad05e2be1`
(`codex/phase7-experiment-305`), read with `git show` only. The retained
[evidence review](checks/s8-specification-20261008/04-review-evidence-proof.md)
lists every artifact read in full, every artifact read in part or not at all,
the raw run directories that were unavailable, and the per-cell numbers.
Nothing was regenerated or rerun.

### 2.1 Shared envelope of every #305 record

Linux `6.12.76-linuxkit` ARM64 in a Docker VM shared with other owners;
`docker run --privileged --network none`; command `/bin/bash -o pipefail -c`
as uid/gid 1000; setup by byte copy; cache contract "sync + VM drop_caches=3,
no residency proof"; `admission_eligible: false`; one record per cell. Status
rule: `OK` if verified and mount+exec+unmount at most 15 s; `SLOW` above;
`FAILED` on a verification miss or a verifier above 10 s; hard stop 60 s, or
600 s for seven cases by owner amendment.

### 2.2 Eligibility matrix

Arms: `L` the S8 product mount; `N` native ext4 at a new identity; `P` a newly
authorized passthrough under the promoted profile at a new identity (P-4).

| Historical row | Source and backend | Oracle | Budget | Actual verdict | Reusable context | Disqualifying differences | Matched arm and prerequisite |
| --- | --- | --- | --- | --- | --- | --- | --- |
| #305 Stage A | Three binaries/images; native ext4, passthrough A1 (zero lifetime, direct I/O), passthrough A2 (60 s, KEEP_CACHE); ARM64 | Exit and stdout hash; scoped tree for mutating cases, read from the ext4 backing directory after unmount | 15 s line; 60/600 s stop; verifier 10 s | 51 records: 36 `OK`, 15 `SLOW`; E09 `NOT_RUN` (nondeterministic output) | Workload bodies; request totals; fresh-mount cost shape; native floor | No Store, no Commit; passthrough persistence; polling timer; no residency proof | `N` + `L` (+ `P`); per workload in section 7 |
| #305 Stage B | Throwaway four-table SQLite overlay prototype on the A1 profile | As above plus a survival read through a fresh mount inside the 10 s verifier | 60 s stop | E01/F `OK`; **E10/F `FAILED`: "separate verification exceeded 10 s"**; the rest `NOT_RUN` | Only that a through-mount whole-layout verifier of the full tree can miss 10 s | Not the promoted profile; no canonical Store; verdict FAILED | None. Never an arm or a control |
| #305 Stage C | — | — | — | Not started; C01–C08 `NOT_RUN` | — | — | Whole lifecycle with Commit: S10 |
| #305 A2 follow-up contracts (narrow enumeration, readdirplus, walk, no-permission, stateless, pin, negative lookup, identity, identity churn) | Passthrough variants, several without permission enforcement | Scoped tree or stdout | 60/600 s | As recorded per contract; `OK` and `SLOW` cells stay as they are | Request-count deltas per mechanism: the source of the candidate hypotheses | Unpromoted arms; permission removal; pinning is a harness setting; declared interference in one window | Hypotheses for `L` only |
| #305 `cf-fsbench-v1` | Empty base | Full-tree hash | 60 s | 60 `OK` | None: resolution-limited | Polling timer | Superseded by v2 |
| #305 `cf-fsbench-v2` | Empty base; event-resolved timer | Full-tree SHA-256, complete | 60 s; host 0.33–1.35 s | 60 `OK`; network rows `NOT_RUN` | The only correctly timed fresh-mount floor (passthrough mount 17–20 ms, unmount 5–18 ms) | Empty base; no Store; some arms without permissions | `N` + `L` (+ `P`) on an empty or declared small root: S8 only |
| #306 computerd 0.4.0 | x86-64 binary under emulation; SQLite | Content proof skipped | 15 s, no exceptions | 7 + 6 `INELIGIBLE`; 11 + 3 `EXCEEDED`; timestamp fidelity `FAIL`; 2 `NOT_RUN` | Budget caution only | Emulation; timestamp mismatch; no residency proof | None |
| #306 Drive9 | ARM64; TiDB + MinIO | skipped | 15 s | 4 `INELIGIBLE`; 4 `EXCEEDED`; 3 `FAILED`; 2 `INCOMPLETE: exit0 with filesystem errors`; 19 `NOT_RUN` | Drain can exceed Exec: report drain and Commit beside Exec | Remote backend | None |
| #306 JuiceFS 1.4.1 | ARM64; SQLite + local objects | skipped | 15 s | 16 `INELIGIBLE`; 4 `EXCEEDED`; 12 `NOT_RUN` | The full-tree byte copy alone took 11.83–12.65 s | Different store model; no residency proof | None |
| Pre-S8 F13/F15 binds | Active core, logical bind | Functional | — | Functional `PASS`; no timing admission | Count baselines for H-1 and H-2 | No FUSE | — |
| Pre-S8 F14, resource growth | Active core, direct operations | Functional and count | 15 s | Selected work complete; exact peaks `INCOMPLETE`; acceptance `OWNER_DEFERRED` | Resource-domain inventory shape | Sampled memory, natural cache | — |
| E04 | Retired host-mediated topology | All 16,777,216 bytes | — | `CLOSED`; no speed comparison | Oracle pattern and custody rules | Withdrawn topology | Not reopened |

#306 is not a matched ranking and supplies no arm. Third-party systems are not
S8 arms unless the owner prospectively selects one.

### 2.3 Workloads and their oracles

The historical manifest records mode, kind, size, symlink target and an
optional per-inode SHA-256. It does not record ownership, times, link count or
inode identity, so it never verified hard-link aliases, timestamps or identity,
and its content hashing covered only command-mutable subtrees. For the product
arm a "state after unmount" read of a backing directory does not exist:
terminal unmount discards uncommitted state, so an oracle is either an in-mount
read before unmount (S8) or a Commit and fresh mount (S10).

| Case | Body | Historical oracle | Prospective oracle for `L` | Prerequisite |
| --- | --- | --- | --- | --- |
| E01 | `true` | stdout hash (proves nothing about the tree) | FP-1 ownership oracle; this is the fresh-mount floor | S8 |
| E02 | `find . \| wc -l` | count only | Full name and kind manifest | S8, full root in a Store |
| E03 | `tar -cf - . \| wc -c` | byte count only | Content digest of the stream | S8, full root |
| E04 | `git status --porcelain` | stdout; scoped tree in follow-ups | In-mount scoped tree; note it may rewrite `.git/index` | S8, full root |
| E05, E06, E07 | `git log`/`diff`, `git grep`, `node -e require` | stdout | stdout | S8, full root |
| E08 | TypeScript build | stdout and scoped tree | In-mount scoped tree | S8; survival S10; see budget conflict |
| E09 | `node --test` | none | none | `NOT_RUN` unless P-7 grants a normalized oracle |
| E10 | edit, `git add`, `git commit` | stdout and scoped tree | In-mount scoped tree; survival through Commit | S8 in-mount; S10 for survival |
| E11 | `git checkout` back and forth | scoped tree | In-mount scoped tree | S8; survival S10 |
| E12 | dependency copy replay | scoped tree, 95,021 entries / 2,126,509,110 bytes hashed | Same scope plus metadata | S8 on the reduced base; survival S10 |
| E13 | dependency hard-link replay | as E12; alias relation not verified | Add the (device, inode, link count) equivalence classes | S8; survival S10 |
| E14 | dependency removal | tree | tree | S8 |
| E15 | large copy and `cmp` | one file | one file, full bytes | S8 |
| E16 | 10,000 unbuffered 100-byte appends | one file | one file, full bytes | S8 |
| E17 | create/unlink churn | tree | tree | S8 |
| E18 | `git status` after an earlier mount | stdout and scoped tree | Labelled "unrefreshed index" before S10 | S10 for the fast path (P-7) |
| E19 | E18 with 282 of 14,090 files changed between mounts | same | Expected output re-derived for the product's change mechanism | S10, or two host-prepared related roots, declared |
| C01–C12 | Cloudflare fs-bench bodies on an empty base | full-tree SHA-256, complete | same | S8 only |

## 3. Functional proofs

Each row is a through-mount native proof on the product, deterministic, with a
bounded wait and an explicit wall stop. "Root" is a declared functional root
appropriate to the row (the existing mixed installed root with symlinks and
hard links, or a wide or large root from the complete-root proofs); it is never
presented as a reduced performance substitute for the full fixture.

| ID | Proof | Passes when | Spec |
| --- | --- | --- | --- |
| FP-1 | Fresh lifecycle: installed Store, existing overlay owner, `Mount`, `Attach`, stat and read through the mount, `/bin/bash -c true`, `Unmount` | `Ready` only after the kernel mount, handshake and loops exist; the command runs as the Bash identity; after `Unmounted` the mount is absent, every loop joined, gauges zero, no surviving session thread or buffer | I-3, I-14 |
| FP-2 | Negotiation receipt | Selected flags and limits equal §8.1; forbidden flags absent; observed maximum READ and WRITE sizes at most 131072; mountinfo options and fusectl values agree; page size recorded | §8.1 |
| FP-3 | Complete-root read | Names, kinds, modes, sizes, symlink targets and link-count classes through the mount equal the root's independent manifest; content digest equal, complete for the declared root | I-1 |
| FP-4 | Identity across two fresh mounts | Identical inode number, size, mtime, ctime, mode, uid, gid and link count for every entry; differing `st_dev` | §9 |
| FP-5 | Confinement | From Bash: the Store and overlay paths are absent and unreadable through `/proc` aliases; the descriptor table holds exactly the three pipes; the connection cannot be aborted or unmounted; a sibling Workspace's mount is absent from the namespace | I-13, §10.2 |
| FP-6 | Streams | Output far larger than any buffer arrives complete and ordered per stream; a stalled consumer stalls the command without growth in daemon-owned bytes; a command longer than every control deadline is not terminated | I-11, §4.1 |
| FP-7 | Independent Exec events | A descendant holding stdout delays that EOF but not `Exited`; a descendant outliving the shell delays `Quiescent`; normal unmount is `Busy` until then | I-12, §5.4 |
| FP-8 | Parked requests | With both loops having received requests that park (external holding fixture), an unrelated request on the same mount and on another Workspace completes | I-8 |
| FP-9 | Background head-of-line | With one readahead READ parked, a second process's cold cached read of another file is observed queued in the kernel, and completes once the first is answered; foreground requests proceed meanwhile | §8.2 |
| FP-10 | Write, append, read coherence | A cached reader and a mutator over write and `O_APPEND` writes: cached read, fresh open, stat and daemon truth agree, with zero notifications and request counts showing the cache was used | §8.3 |
| FP-11 | Create, unlink, replacement rename, negative lookup | Each is visible immediately through cached dentries; an `ENOENT` is not cached | §8.3 |
| FP-12 | Lookup against directory mutation | LOOKUP of a name racing CREATE, UNLINK and RENAME of that name in the same directory never returns a stale binding; confirms the directory-lock dependency of the single-job read | §6.4 |
| FP-13 | Truncate, shrink, regrow, `O_TRUNC` | No discarded tail byte reappears in cached pages or fresh reads | §8.3 |
| FP-14 | Hard-link aliases | A write through one name is read through the other from cache; link count and ctime after unlink are correct without waiting for a lifetime to expire | §8.3 |
| FP-15 | Shared mappings | `msync`, `munmap` without `msync`, and map–close–store–exit: observed WRITE flag, header identity, handle liveness and RELEASE order; no ctime-bearing SETATTR; a store through a mapping while another process holds an append handle lands at its own offset; a store past EOF in the last page followed by an extension is compared between cache and daemon; Commit inclusion is exactly the published frontier | §8.3 |
| FP-16 | Reply ordering | A GETATTR held across a WRITE (external fixture) is discarded by the kernel; size never shrinks during concurrent append and read | I-6 |
| FP-17 | Permissions, times and symlinks | A mode-000 file is unreadable from cache; `chmod` takes effect at once; ctime equals mtime after `chmod`, link and rename; symlink targets up to the page limit round-trip; longer creation is refused | §8.3, §9 |
| FP-18 | Refused requests | After the first `ENOSYS`, no further xattr request arrives across many small writes; special files, set-id bits and ownership changes are refused with the stated errno; `fsync` succeeds with no work | §6.4, §8.4 |
| FP-19 | Cancel while parked | A process blocked in a parked request receives its reply before the daemon waits for its exit; `ExecCancel` then observes `Exited` | I-7 |
| FP-20 | Unmount Busy | With an open handle, a current-directory holder, or a live descendant: `Busy`, the Workspace fully usable, and a later unmount succeeds | §11.1 |
| FP-21 | Detach evidence | After `umount2` returns 0 every loop observes `ENODEV` and is joined; with `K` kernel lookup references outstanding and no FORGET, and with handles the kernel never releases after an abort, the namespace still reaches `Gone` | §11.2 |
| FP-22 | Propagation | A command in its own namespace keeps the mount busy for the daemon's `umount2`; after the command exits the unmount succeeds and no copy remains; a long-running command in Workspace A does not keep Workspace B's connection alive | §10.3 |
| FP-23 | Forced teardown | Execs signalled, parked replies completed, connection aborted, loops joined; retained Commit custody is reported as unknown publication, never as not published; a step that cannot be established yields `Retained` with its phase | §11.3 |
| FP-24 | Several Workspaces | Simultaneous mounts over different and related roots: correct routing, isolated mutable state, a stale token never redirects | I-2 |
| FP-25 | Lost replies | Lost `Mount`, `Attach` and Exec connection (external fixture): the entry is observable through `Locate`/`ExecStatus`; no second binding, attachment or execution | §12 |
| FP-26 | Debt and admission | Repeated mount, write, unmount faster than reclaim while a peer writes: closed-namespace count and database pages plateau, or `Mount` is refused with the typed capacity result; a stopped maintenance turn is visible in status and refuses new mounts | §11.4 |
| FP-27 | Reader health and failure scope | A quarantined reader receives no further demand; one request's cold failure does not fail a later request | §7.2 |
| FP-28 | Open-unlinked and rotation | A retained descriptor keeps exact content across unlink, rotation and truncation; rows are released at the last close | §5.3 |
| FP-29 | Removed directory without a handle | A request naming it returns either its retained attributes or `ESTALE`, never another inode's data | §5.3 |

Adversarial races that must be included rather than sampled around: reply
ordering (FP-16), admission against unmount (FP-20), cancellation against a
parked reply (FP-19), a flight subscriber's cancellation if coalescing is ever
selected, and attach failure after `mount(2)` returned 0 (FP-25).

## 4. Count hypotheses

Frozen before any timing. Each is falsified by a single contrary receipt. Counts
come from existing receipts (`StoreWork`, `ClientWork`, `OwnerWork`, `JobWork`,
session counters) plus the request-service and mount-session gauges the plan
adds.

| ID | Hypothesis | Falsified when |
| --- | --- | --- |
| H-1 | A fresh-cache mount's object batches follow the tree depth (the recorded 10/11/12 shape for 1/1024/100000 files) and include no enumeration | Batches grow with entry count beyond depth, or any whole-directory or whole-tree read appears |
| H-2 | A warm fresh mount performs zero object demands, exactly one history read and one overlay Open job, and creates no database | Any is larger |
| H-3 | Attach performs a fixed number of system calls, starts a fixed number of threads and makes no base demand | A base demand or a variable count appears |
| H-4 | A read-class request executes exactly one owner job, zero write transactions and zero base-source leases | More than one job, or any write transaction |
| H-5 | Baseline to record: object-cache hits and length demands per warm LOOKUP and GETATTR of an unchanged base entry | (Recorded, not gated; it is the gate input for the fact-cache candidate) |
| H-6 | A mutating request executes one mutation transaction and one ticket-release job | More |
| H-7 | OPEN and RELEASE execute one lifecycle job each | More |
| H-8 | FORGET executes zero jobs and zero SQL; an unmount with `K` outstanding lookup references runs a constant number of foreground jobs | Work grows with `K` in the foreground |
| H-9 | At most `max_background` background requests of a mount are in userspace at once | The observed depth exceeds it, or a foreground request is blocked by a parked background one |
| H-10 | No dispatch loop and no service worker is ever blocked waiting for an owner job, a Store reader, a flight or an owner-admission credit. The only loop wait is for the mount's own request credit before its next read (specification §6.2) | A thread-state observation finds any other wait |
| H-11 | Cold demands keep the caller's batch (no per-identity Store calls); batch identities and bytes are recorded against the demand window; duplicate concurrent acquisitions of one identity are counted; a hot Workspace's demand waits at most the declared number of in-service batches while a peer scans | A split batch; an unbounded wait |
| H-12 | Under sustained unequal job weights every admitted runnable Workspace completes at least the declared share of requests | A Workspace falls below it |
| H-13 | Baseline to record: bytes copied, allocated and freed while the cache mutex is held, and cache wait, under several concurrent Workspaces | (Recorded; gate input for shared cached bytes) |
| H-14 | S10: a second fresh mount's `git status` issues no content READ for unchanged tracked files once a refreshed index is committed | Any such READ |
| H-15 | After terminal unmount the mount's threads, owned requests, cursors and receive buffers are released and per-mount gauges are zero | Any remains |
| H-16 | Reclaim debt of a closed namespace reaches `Gone` in bounded steps with no further API call, while other Workspaces run and when idle | Debt stalls without a reported cause |
| H-17 | FUSE requests by opcode for each workload fall in a range predicted from the profile before the run | Outside the range |
| H-18 | After the first `ENOSYS`, zero xattr requests | Any |

For any SQL hypothesis the future proof must carry the exact
`EXPLAIN QUERY PLAN` (full `EXPLAIN` where needed) together with correlated
runtime statements, VM steps, rows, pages, copies and waits. Wall time or a plan
alone is not attribution, and no SQLite cost is to be attributed from the failed
prototype's wall time.

## 5. Resource domains

Reported separately, never summed into one number, with sampling limits stated.
A sample maximum is not a continuous peak; a lifetime high-water mark is not a
phase peak.

| Domain | Observation |
| --- | --- |
| Daemon heap and resident set | Process observations at phase boundaries |
| Immutable cache | Retained charge; lent bytes if shared ownership is selected |
| Demand and decode transients | Read-service gauge; concurrent demands |
| Owned request bytes and replies | Per-mount gauges against `R` |
| Overlay SQLite | Pager and journal observations; logical and allocated file bytes |
| Store | Writer and reader session counters; logical and allocated file bytes; WAL size |
| Per-mount native | Threads; receive-buffer address space and resident pages; handshake buffer; cursors |
| Kernel | Page, dentry and inode cache attributed through the cgroup; never credited to or hidden behind a bounded heap |
| Reclaim debt | Closed namespaces; queued rows; maintenance state |

## 6. Cache classes and comparison arms

Three classes, never pooled. Every arm of a comparison is in the same class
with the same enforced state.

| Class | Definition | Enforced by | Cross-arm use |
| --- | --- | --- | --- |
| A — fresh mount, cold daemon cache | New daemon or proven-empty immutable cache, new Store connections, new overlay, new kernel connection | Store file and sidecar residency measured before the attempt by the mechanism P-5 selects; nonzero is `INELIGIBLE` with zero attempts; a cache drop alone is not proof | `N`, `P`, `L` |
| B — fresh mount, warm daemon cache, fresh kernel connection | A declared untimed identical call in an earlier mount of the same daemon, then terminal unmount | The measured phase's own receipt shows zero object demands (H-2); Store-file residency for the still-paid history read is declared | Within `L` only: native ext4 has no analogue |
| C — same mount, warm native caches | A declared untimed identical call on the same mount | Opcode counts; the interval between warm-up end and measured start is recorded and below the 60 s lifetime, otherwise the sample changes class and is `INELIGIBLE` | `N`, `P`, `L` |

For every phase the report names the content residency of the Store and overlay
files, the SQLite and reader caches, the daemon immutable cache and the kernel
metadata and page caches, and identifies natural own-write and setup effects.
Prepared inputs may be reused through declared byte-copy clones; timed product
work stays inside the measured operation. A retained mount is never charged a
fresh mount per call, and a fresh mount is never amortized away.

## 7. Scenario matrix and priced boundaries

| Scenario | Classes | Dependence |
| --- | --- | --- |
| Sequential per-call churn: mount, Exec, unmount, repeated with a fresh identity each | A, B | S8 |
| Related roots and branches | A, B | S8 when the roots come from host Init; S10 when from Commit |
| Simultaneous Workspaces on one daemon | A, B, C | S8 |
| Several Bash commands on one Workspace, sequential and concurrent, short and long-lived | C | S8 |
| Shared cold misses; a subscriber's cancellation or unmount | A | S8 (coalescing rows only if selected) |
| A scanning Workspace against a hot one | A, C | S8 |
| Tiny and dispersed writes; fragmentation | C | S8 |
| Hard links and rename | C | S8 |
| Truncate, regrow, mappings | C | S8 |
| Log rotation and open orphans | C | S8 |
| Terminal detach and drain with live descendants, descriptors and dirty mappings | — | S8 |
| Repeated incremental Commit on one mount; Commit and remount survival | — | S10 |
| The full historical fixture | A, B, C | S8 plus a faithful host acquisition of it into a sealed Store, which no retained artifact shows has been done |

Priced boundaries, recorded separately and balancing to the complete command
without summing overlapping spans:

```text
 setup (clone or declared reuse)  |  not product time
 mount: Mount + Attach to Ready   |  product
 Exec: launch .. Exited, output drain to both EOFs, Quiescent   (three separate marks)
 explicit Commit when selected    |  S10
 terminal unmount: fence .. Unmounted (detach, join, logical close)
 eventual cleanup: Unmounted .. Gone   (reported; never hidden, never charged to the reply)
 independent verifier             |  separate command and budget
```

Speed and storage are reported together at pinned source, product, build,
binary, image, harness, workload and cache identities: Store and overlay
logical and allocated deltas beside every time. A change that costs about half
the speed for about five percent of storage is rejected; raw deltas are
reported either way.

## 8. Budgets and pending owner decisions

No threshold is proposed and no bound is changed here. These are the conflicts
the retained evidence predicts, each a choice between keeping `NOT_RUN` with the
conflict stated and a prospective scoped exception. They are specification
choices P-3 to P-7. The owner ruled on all of them on 2026-10-08; section 8.1
records how each row is resolved.

| Conflict | Retained observation | Decision needed |
| --- | --- | --- |
| Full-fixture setup | Of 128 full-fixture #305 records, the complete host command ranged 14.6–122.0 s and one was at or under 15 s; `true` alone cost 16.3–18.3 s on the Stage A arms with under 0.07 s in the container; the byte copy alone was 10.36–12.65 s in #306, which recorded `NOT_RUN` rather than enlarging the budget | Complete-command scope for full-fixture samples (P-3) |
| Shared sealed Store | No full-fixture sealed Store exists; its size and copy time are unknown | Whether a read-only sample may bind one closed Store without a per-sample copy, under a before/after identity proof including sidecars and a per-class residency proof (P-3) |
| Long workloads | Native Exec alone: E08 71.5–78.1 s; E12 10.2–12.0 s; E13 12.2 s; E03 8.8–9.2 s. Passthrough: E12 27–44 s, E13 58.6 s, E03 18.8–28.3 s, E14 16.3–16.8 s | Exec bounds for these cases, or `NOT_RUN` (P-3) |
| Verifier | Scoped oracles on ext4 already cost 7.3–9.3 s (E12), 8.6–9.6 s (E13), 7.6–8.3 s (E08); the only through-mount full-tree verifier exceeded 10 s | Proof bound, or an explicitly labelled scoped oracle (P-3) |
| Controls | Old numbers cannot be arms; the experiment tree is throwaway | New `N` and `P` identities, and what "materially better" compares against (P-4) |
| Residency | The VM is shared; a VM-wide cache drop touches the protected containers and proves nothing about residency | Accepted residency mechanism (P-5) |
| Background depth | The promoted profile fixes it at 1 | Whether a single-mechanism arm may vary it (P-6) |
| E09, E18, E19 | No admissible oracle (E09); S10 dependency (E18, E19) | P-7 |

The old #305 60 s and 600 s caps and the host-history 300 s and 30 s exception
keep their original scope and authorize no new S8 bound.

A "materially better" claim, when one is made, must state the two arms, their
class, the complete user-visible boundary it covers and the storage delta. This
plan defines no numerical threshold for it.

### 8.1 Owner rulings, 2026-10-08

The owner ruled on P-1 to P-7 on 2026-10-08. The rulings are in
[specification section 15.3](S8-SPECIFICATION-20261008.md#153-owner-rulings-2026-10-08);
this table says how each conflict row above is resolved. Where a ruling leaves a
number to registration, none is set here.

| Conflict | Resolution |
| --- | --- |
| Full-fixture setup; shared sealed Store | A sample binds one closed sealed Store without a copy only when its own receipt shows zero serial reservations and zero Store write transactions, under a before/after identity proof of the Store file with a declared account of its sidecars and its class's residency proof; otherwise it is `INELIGIBLE` for shared binding and uses a declared byte-copy clone. The clone budget waits for the fixture Store's recorded size and copy time |
| Long workloads | E08 `NOT_RUN`. E03, E12, E13 and E14 are priced diagnostic comparisons against `N` at the same identity and class: no pass/fail budget, no admission verdict, an explicit wall stop declared at registration |
| Verifier | Scoped oracle, labelled scoped, on timing rows; the full-byte oracle once at final identity as a functional proof under its own declared exception |
| Controls | `N` and `P` are both authorized at new prospective identities; `P` is harness code. "Materially better" compares `L` against `P` in the same class over mount + Exec + unmount, with the storage delta and the gap to `N`. The owner sets the threshold after the first diagnostic data |
| Residency | Per-file eviction hint, then a per-file residency measurement of the Store file and sidecars before the attempt; nonzero is `INELIGIBLE` with zero attempts; no VM-wide cache drop |
| Background depth | One prospectively registered single-mechanism arm may vary it after FP-9; 1/1 stays the default |
| E09, E18, E19 | E09 `NOT_RUN`; E18 only as a labelled "unrefreshed index" case; E19 deferred to S10 |

Oracle added by the P-1 ruling, run in the mutation checkpoint with an external
peer that really holds the Store writer: a create that finds the local serial
range exhausted and whose single reservation attempt meets `Busy` returns
`EAGAIN`, leaves no effect, and makes no second attempt; a later create
succeeds once a reservation succeeds. Count to record with it: reservation
attempts per create are at most one, and early attempts begin only below the
configured low-water value.

## 9. Registration and custody

- A selection is registered prospectively in the core harness registry with its
  source, product, compilation, dependency, binary, image, harness, workload and
  cache identities before any sample; a dirty tree is never a sealed arm.
- Functional proofs are external tests and examples against the production
  library that normal consumers use; no test-only product hook and no
  benchmark-selected behaviour.
- Unaffected qualifying proof is reused by exact identity; a closed passing
  treatment is never resampled unchanged. A newly authorized control has its own
  prospective identity and justification.
- Protected containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59` and
  `d2550144998b`, every unrelated process and worktree, and every historical
  receipt are left untouched. Store files live on a named in-VM volume or a
  native container filesystem, never under the repository bind mount.
- Original failures, ineligible rows, unknown outcomes and their custody are
  retained in the campaign-owned ledger exactly as observed.
