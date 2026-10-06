# 03 — Commit workflow

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal; no opcode, schema version or
> persistence guarantee is allocated here.

> **Superseded 2026-10-05** by
> [`../303/04-concurrency-commit.md`](../303/04-concurrency-commit.md).
> Publication is `stage_changes` then `commit_staged`, not one conditional
> transaction, and there is no object store. The text below is unchanged and is
> kept as context.

## 1. Shape

```text
        Exec keeps running: writes go to active generation g+1
────────────────────────────────────────────────────────────────────────────────>
 admit   capture        construct + encode + upload + register        publish  install   retire
   │       │  ┌───────────────────────────────────────────────────┐     │        │        │
   └──────>└─>│ page frozen rows → C1 → C2 → object store / global │────>└───────>└───────>└──> …
              └───────────────────────────────────────────────────┘
  write lock:  ▓ (1 stmt)              none                          none*     ▓ (1 stmt)  ▓ small batches
```

`*` Publication is one short transaction on the **global** metadata store; it
does not touch the overlay database, so it cannot delay a FUSE write.

## 2. Phases

| Phase | Work | Runs on | Overlay write lock | Bound |
| --- | --- | --- | --- | --- |
| **Admit** | Refuse if this Workspace already has a pending Commit | daemon | no | constant |
| **Capture** | `frozen_gen = active_gen; active_gen += 1`; record base, Branch and expected head as the capture context | overlay DB | one statement | constant |
| **Construct** | Page dirty inodes of the frozen range; for each changed file hand C1 the base file root plus replacement runs read from `block` rows; for the namespace hand C1 the changed inode and name rows | daemon, one construction worker | no | bounded input and output windows |
| **Encode / pack** | C2 exact-reuse check, FULL/PREFIX/STORED selection, pack assembly | daemon | no | existing C2 batch and pack limits |
| **Upload** | Payload packs: put-if-absent to the object store | daemon → S3 | no | bounded in-flight window, backpressure to C1 |
| **Register** | Metadata bodies and locators: idempotent inserts, in bounded batches | PostgreSQL | no | existing C2 transaction row/byte limits |
| **Publish** | One conditional transaction: Branch head `expected → new`, Commit and Layer records | PostgreSQL | no | constant |
| **Install** | `base_root = new; folded_gen = frozen_gen; frozen_gen = NULL` | overlay DB | one statement | constant |
| **Retire** | Delete rows with `gen <= folded_gen` | overlay DB | short batches | bounded rows per batch |

Construction carries one **capture context** (Workspace, incarnation, generation
range, base Commit and root, Branch, expected head, storage profile) from capture
to install. No later step reconstructs those facts.

## 3. Ordering invariants

1. **Bytes before references.** A pack is acknowledged by the object store before
   any locator naming it is registered. A locator and every delta base it depends
   on are registered before the root that reaches them is published.
2. **One publication.** Readers of the global metadata store see either the old Branch
   head or the new one. Registered-but-unpublished objects are unreachable, not
   half-visible.
3. **Install only what is known.** The base advances only after publication is
   known. Install is compared against the capture context (`frozen_gen = ?`); a
   stale or repeated install changes nothing.
4. **No transaction spans waiting.** No overlay or global transaction is open
   across construction, encoding, an HTTP request or command execution.
5. **One pending Commit per Workspace**, held until its outcome is known.
6. **One construction worker** (root `AGENTS.md` §3.8). `LAYERFS_CONSTRUCTION_WORKERS=1`;
   no helper lane is added to pass a gate. Initialization keeps its own
   multi-worker exception.

## 4. What a file's Commit input looks like

For a changed regular file, the frozen rows give exactly the final state:

- final `size` and `inherit_len` from the inode row;
- replacement runs = maximal sequences of consecutive `block` rows;
- zero ranges = positions at or beyond `inherit_len` with no block row;
- everything else = unchanged ranges of the base file root.

That is the input C1's existing edit and stream constructors take
(`apply_edits`, `construct_stream`). Overwritten history is never replayed: a
block rewritten a thousand times contributes one run. A new or fully rewritten
file streams as fresh content. C1's chunk boundaries, split/join policy and
identities are unchanged; block boundaries are a private detail of the overlay.

## 5. Outcomes

Every step ends in exactly one of four outcomes. There is no silent retry, no
alternate route and no guessed cleanup.

| Outcome | Meaning | Overlay effect |
| --- | --- | --- |
| **Published** | Conditional transaction committed | Install, then retire |
| **Conflict** | Branch head is not the expected one | Clear `frozen_gen`; Workspace keeps all changes on its old base. No automatic rebase |
| **Refused** | Definite failure before publication (integrity, capacity, provider refusal) | Clear `frozen_gen`; changes kept; uploaded packs are unreferenced garbage |
| **Uncertain** | A request's result is not known (timeout, lost response, process death) | Keep `frozen_gen` and custody; refuse another Commit until resolved |

**Resolving Uncertain (D7).** Immutability makes resolution an exact question
rather than a guess:

- *Did this pack arrive?* The key is the digest of the body. `HEAD` answers it,
  and a repeated put-if-absent of identical bytes cannot change stored state.
- *Did this publication happen?* The global store is a network server, so the
  reply to a committed transaction can be lost, and the daemon can die before
  reading it. The pending capture context names the candidate root and expected
  head; whether the Branch history contains that candidate is a point lookup.

Phase 6 prohibited "query-based guessed adoption", and `core/AGENTS.md` says an
unknown persistence outcome is a failed result. With a network metadata store an
uncertain publication is an ordinary event, so an **exact identity lookup** is
required to make progress. The proposal is to permit exactly that and still
prohibit anything inferential (time windows, "probably mine", automatic resend).
This needs an explicit owner ruling and a rule amendment.

Whether an idempotent put may be *retried automatically* is a separate policy
question. The recommendation is to keep one attempt per operation and make the
recovery path an explicit new Commit, which finds already-uploaded packs through
normal exact reuse.

## 6. Failure matrix

| Failure point | Global state | Object store | Overlay | Next step |
| --- | --- | --- | --- | --- |
| During construct | unchanged | possibly some packs | frozen kept | Refused → clear `frozen_gen` |
| Upload error (definite) | unchanged | partial | frozen kept | Refused |
| Upload timeout | unchanged | unknown for that pack | frozen kept | Uncertain → `HEAD` |
| Register error | maybe some locators (unreachable) | complete | frozen kept | Refused |
| Publish: head moved | unchanged | complete | frozen kept | Conflict |
| Publish: reply lost or timed out | unknown | complete | frozen kept | Uncertain → lookup → install or Refused |
| Daemon dies before publish | unchanged or unreachable rows | partial | frozen persisted | Lookup → not published → Refused |
| Daemon dies after publish, before install | published | complete | frozen persisted | Lookup → published → install |
| Install statement fails | published | complete | frozen kept | Retry install (idempotent on `frozen_gen`) |

Unreferenced packs and unreachable locators left by a refused Commit are
harmless and are removed only by reachability-based collection, which is out of
scope for the first slices ([04 §8](04-storage-interfaces.md#8-deferred)).

## 7. Concurrency

- **Same Workspace:** Exec and Commit overlap; a second Commit is refused while
  one is pending.
- **Different Workspaces, one daemon:** independent captures and independent
  construction; they share the overlay database's single writer for short
  transactions only. Aggregate memory, upload windows and the construction-worker
  rule across Workspaces need their own admission design (#249).
- **Different daemons:** construction and upload are fully independent. They
  meet only at short global transactions; two Workspaces committing to the same
  Branch produce one Published and one Conflict.
