# R1b Save/index/codec pre-effect working ownership

> Prospective delivery, 2026-10-01, against published parent
> `53b6bf741a5693f4d00ec98b914ce34645ee9ab3` plus root-coordinated R1 source.
> Actual allocation/owning checks remain unrun. This is a scoped foundation;
> original read/consumer/cache/global/physical gates remain mandatory and open.

SC-02/03/04/07/08 require pre-effect resource admission, same-Store association
and known/Unknown custody. Owned files are new cas/working*.rs, cas/{store,
owner,lifecycle,mod}.rs, external save_working tests, this freeze and storage
architecture. Root's existing Store::same_authority method remains intact.

One continuing native arbitration Arc defines the actual Store authority; a
bounded weak association registry compares Arc identity, never paths or public
Store IDs. Independently opened aliases share one scoped ledger. Persisted Save
SQL/default writer count2/configured slot space remain authoritative across
handles/processes. The local byte ledger complements them and cannot replace
their admission or provide a cross-process/global memory qualification.
The persisted4/64 values remain SQL admission limits. The new byte-admitted
same-process72MiB profile can refuse a third Save before those SQL slots fill;
production byte-refusal counters distinguish that cause. Historical4/64 source
proofs retain their original identity/scope and are not relabeled by this change.

The scoped ledger is176MiB for Save reservations and real shared-index copies
only. Each opened handle's actual shared index pair receives8MiB before index
load/allocation, and that credit follows every actual last shared-index owner.
Two standard72MiB Saves plus one8MiB index pair reserve152MiB. One96MiB
Singleton plus one72MiB standard and one8MiB index pair reserve176MiB. Extra
aliases consume real additional index credit; capacity refusal precedes further
index allocation or Save connection effects. This arithmetic excludes ordinary
read owners, other Stores, Server state, engine/cache/physical memory and cannot
enable StrictServerMemory/global capabilities.

Standard72MiB keeps the original total:17MiB codec,16MiB private/publication
index overlap,12MiB wave/group/pack,8MiB C1 state,12MiB provider/results,
1MiB bookkeeping and6MiB remaining margin. The earlier8MiB index/14MiB margin
proposal is explicitly redistributed inside the unchanged72MiB. Each of the
private and prospective publication index pairs has8MiB pre-admitted allowance.
Actual compiled/real requested B-tree allocations must fit; nominal
PoolIndex::live_bytes (tuple size plus8) is not that proof. The Singleton class
is the separately declared96MiB prospective owner. No native/worker/deadline or
configured writer-count increase occurs.

Store::begin_save selects standard reservation and an explicit
begin_save_with_class selects the declared class before connection/open/SQL.
These are reservation foundations, not an assertion that every legacy raw Vec
consumer/16MiB singleton combination already fits a fully enforced72MiB profile.
They neither shrink the existing canonical/read compatibility limits nor use a
failed capacity call to switch algorithm/class.

Save reservation occurs before opening its connection. Codec and private-index
allocation precede persistent Save birth, eliminating preparation-failure cleanup
of a newly born row. A COMMIT-Unknown birth retains the actual connection,
prepared index/codec owners and reservation in a fixed authority slot, with no
rollback/close/refund on Rust Drop. Acquired Unknown or failed cleanup similarly
moves the real MutationOwner into its reserved slot. Known success/cleanup frees
actual pending bytes/index/codec/native connection before returning credit.

Prospective publication index copies are prepared while the acknowledged Save
still owns its reservation and before final SQL COMMIT. Short shared-index guards
cross COMMIT/known cache assignment; no mutex spans upload/input/construction.
After known publication, ready copies move into already funded shared ownership;
old shared allocations are actually dropped. No clone allocation follows final
publication. Failed or unknown publication retains its real owner/credits.

Actual production status exposes class/current reservations, active/retained
owners, shared-index credits and known native Save IDs where acknowledged.
Uncertain birth never obtains a guessed Save ID from a later query. Original
typed failures return once; status retains original failure text and actual
resource custody. Registry/slots stay bounded; no retry/adoption/repair is added.

Required owning exits: actual hardlink/independently opened aliases share the
ledger; two standard Saves retain separate persisted slots; pre-effect byte
exhaustion leaves SQL/connection/index/codec effect counts unchanged; explicit
class arithmetic and successful last-owner refund; real populated B-tree/private/
publication copies and codec allocations observed through System; real SHARED
reader Save-birth/publication COMMIT Unknown and failed cleanup retain charged
connections/owners; existing canonical storage semantics remain unchanged.

Raw AuthenticatedObjects/Store/Save Vec results cannot carry credit through C1
slow consumers or cache copies. Current read caches can coexist at10MiB before
outputs and full BLOB Vec capacity survives truncate; the32MiB returned-canonical
compatibility reader is not an8MiB admitted read class. Those genuine owning API,
cache/pack shape, aggregate allocator/native/protected/physical proofs remain
open. This delivery must not relabel them PASS or hide them from R1 closure.


## Retained codec custody correction (prospective source freeze)

First root-serialized copy compile reports REGISTRY !Sync because retained
PreparedSave/MutationOwner contains raw CCtx/DCtx pointers. Underlying owned
codec workspaces are the actual unsafe boundary: each private pointer targets
its own eight-byte-aligned Vec<u64> arena; moving Vec preserves its allocation
address, no method resizes it after initialization, no pointer is exposed, all
context calls require exclusive &mut and are blocking with nbWorkers0. Prefix
references are cleared after the call on success/failure; reuse first resets
parameters. Drop only frees its owned Rust arena and invokes no context operation.
This permits Send for these two workspace owner types only, not Sync and not a
blanket sibling/capsule/authority implementation. Mutex protects shared retained
custody; no concurrent context execution or extra worker is enabled.

Audited published source: locked zstd-sys2.0.16+zstd1.5.7 lib/zstd.h context and
static-allocation contracts (caller retains an immovable allocation, serial use),
and existing zstd-safe7.2.4 CCtx/DCtx Send boundary with all unsafe operations under
mutable borrow. Primary references: https://github.com/facebook/zstd/blob/v1.5.7/lib/zstd.h
and https://github.com/gyscos/zstd-rs/blob/zstd-safe-7.2.4/zstd-safe/src/lib.rs .
No third-party source is modified or replaced. Exact new external codec_custody
proof creates/uses real contexts, moves both owned arenas between threads, reuses
both sequentially and checks bytes. Only root runs this covering proof.
WorkingAuthority/IndexLease gain bounded manual Debug output (identity/credit),
without locking/traversing quarantined retained codec/native owners.

Owning allocator correction: on macOS Rust1.85.1, the two published index mutexes
created with Store are initialized lazily at their first Save lock. Each owns one
Box<pthread_mutex_t> request64, so Save abort can correctly leave128 captured Rust
bytes with the still-live Store/published-index8MiB credit. The test records exact
remaining requested-size histogram, requires codec/capsule0, then drops actual
Store and requires0. Capture is unchanged; there is no prewarming, excluded owner,
cap change or relaxedzero-refund requirement. Primary pinned source:
https://raw.githubusercontent.com/rust-lang/rust/1.85.1/library/std/src/sys/sync/mutex/pthread.rs .
