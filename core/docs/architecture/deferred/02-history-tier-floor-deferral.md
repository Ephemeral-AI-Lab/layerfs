# Deferred: the retained-history tier floor — grouping, per-path frames, and the one-stream bound

> **Status:** Deferred. Target LayerFS v0.1.7; not a released contract. **Nothing here is implemented,
> scheduled or measured as a design.**

**Tracking issue:** [#189](https://github.com/Ephemeral-AI-Lab/layerfs/issues/189) — *standalone, no
parent*, label `deferred`. It carries the revisit condition and the exit criteria.

This paper records three items the #187 campaign **measured as targets and declined to pursue** while
[T1](../../../../docs/roadmap/0.1/0.1.7/retained-history-t1/README.md) — the per-object-record tier — is
the only tier under consideration. They are recorded together because they are one decision: *how far past T1 is worth
going, and what each step costs in access.*

It is diagram-led for the same reason the [size-transition
paper](01-size-transition-delta-hints.md) is: the three tiers differ in **physical arrangement**, not in
degree, and a single merged picture is how they get confused.

**This paper holds no chapter number.** Chapter numbers are global to the
[replacement-core architecture set](../README.md), and this is not one of its descriptive papers: it
describes no shipped behaviour.

## Claim labels

Reused from the [proposal folder](../proposal/README.md), so the two kinds of guidance read the same way:

| Label | Meaning |
| --- | --- |
| **holds today** | read from source; true of the tree at the pin |
| **measured** | read from a Store, a trace, or the corpus by the #187 campaign |
| **computed** | arithmetic over measured parts, stated in full |
| **est** | one or more inputs are borrowed from a different population |
| **proposed** | does not exist in `core/`; a design to be argued with |
| **open — required** | a prerequisite for something else here |
| **deferred** | explicitly out of scope; an owner amendment must precede it |

## Source pins and method

- Product and harness read at commit `66bce8378`. The #187 campaign's harness change
  (`core/benchmark/fs-bench-pro-storage-content/`) is uncommitted at that commit and is **not product
  source**; production LOC delta **0**.
- Comparison artifact: v0.1.6's retained stride-10 Store,
  `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`,
  49,315,840 B, sha256 `80c2b10a7ca1513228306e42063611e2b716be053023b65073d0ab67fbee50af`.
- Method: source reads plus the campaign's byte measurements. **No build, test, benchmark or code
  change was made for this paper, and it produces no timing number.** The machine was shared throughout
  the campaign, so every access cost below is stated in **bytes per read**, never seconds.
- Every number is **diagnostic**, not admission evidence.

## 1. The case

T1 keeps today's physical grammar: **one record per pack group**, per-object records, the same lanes.
The three deferred tiers each change the *arrangement*:

````text
  T1      per-object records, one per group     cross-path delta bases, lean rows
  T2      T1 + the base-less records GROUPED    several records in one ~256 KiB frame
  T3      PER-PATH CHAINS in large frames       a path's versions contiguous, zstd -19
  floor   ONE STREAM over the whole union       no random access at all
```

````
  128,864,256  registered lane (today)
   63,737,856  the faithful model, as measured              [measured]
   47,048,435  T1 target                                    [computed]  <-- the only tier pursued
   49,315,840  v0.1.6
   ~44,374,000 T2                                           [est]
   ~31,100,000 T3                                           [composed]
   ~21,382,000 floor                                        [composed]
````

## 2. T2 — grouping

### G1 · What it changes

````text
  T1 (today's grammar)                    T2 (grouped)
  +--------+ +--------+ +--------+        +---------------------------+
  | rec 1  | | rec 2  | | rec 3  |        |  one zstd frame, ~256 KiB |
  | own    | | own    | | own    |        |  rec 1 | rec 2 | rec 3    |
  | frame  | | frame  | | frame  |        +---------------------------+
  +--------+ +--------+ +--------+
  one record per group                    many records per group
  read 1 record = decode ~1 record        read 1 record = decode the WHOLE group
````

**holds today:** the machinery exists — `PackLane::Ordinary | Native | PooledMetadata` already run
`GroupCodec::Zstandard` with one frame per group. Multi-record groups are not a new concept.

**holds today:** the whole-file lane seals the moment it is occupied —
`PackLane::WholeFile | PooledMetadata | Singleton => occupied`
(`core/crates/layerfs-storage/src/cas/selection.rs:54-55`). Grouping whole-file records means changing
that arm and revisiting `GROUP_TARGET` and the per-lane body limits.

### G2 · Why it is not simply "turn grouping on"

**measured (B3):** grouping at the product's own 256 KiB pack size is worth **1.2637x** on the lane as a
whole — but split by base presence it is **not** uniform:

````text
  records WITH a prefix dictionary     2.95 % WORSE at 256 KiB
  records WITHOUT one                  1.1875x better at 256 KiB
````

**Grouping does not subsume the prefix dictionary.** It must therefore be applied *selectively* — a
per-record decision or a second lane, not a global switch. **measured:** the cost is **59.8x**
single-record read amplification at the 256 KiB cap (median group 255,391 B, median 29 records).

### G3 · The number, and its weak input

**est:** T2 = T1 minus 2,674,059 B = **~44,374,000 B apparent**. The saving applies B3's measured
1.1875x to the 8,741 whole-file objects that R4 leaves without a base
(48,131,288 B canonical -> 16,935,710 B at 2.842x -> 14,261,651 B grouped).

**The borrowed input:** B3 measured 1.1875x on the *current* base-less population
(25,804 objects / 266,427,536 B). T2 applies it to the *post-R4* population
(8,741 objects / 48,131,288 B) — a **different size mix**. That single borrowed ratio is why T2 is
`est` and not `computed`.

## 3. T3 — per-path chains in large frames

### G4 · What it changes

````text
  T1 / T2 (per object)                    T3 (per path)
  path A v1  +--------+                   +--------------------------------------+
  path A v2  | each   |                   |  path A: v1 | v2 | v3 | v4 | v5     |  one frame
  path A v3  | its own|                   +--------------------------------------+
  path B v1  | frame  |                   +--------------------------------------+
             +--------+                   |  path B: v1 | v2                     |
                                          +--------------------------------------+
  254 frames over per-path chains, zstd -19 --long=30
  mean frame 1,464,553 B; max 11,263,931 B decoded per read
````

**measured (B1):** P1 = **27,184,431 B** for the 371,937,306 B union = **13.682x**, at 254 group frames.
**composed:** scaled to the Store's canonical (x1.0242 = 27,841,060 B) plus a lean non-pack
(3,259,108 B) gives **~31,100,000 B apparent**.

### G5 · Why it is a different design, not a tuning change

- **It needs a concept the Store does not have: a path.** `layerfs-storage`'s contract explicitly denies
  a Workspace or history entity (`core/crates/layerfs-storage/src/lib.rs:5-7`), and there is no path
  anywhere in the schema.
- **It breaks an invariant the whole-file lane relies on.** One record per group is what makes
  per-object stored-size attribution exact (`space.whole_file_records` in the harness). At a median of
  29 records per group that attribution is gone.
- **Codec cost:** level 19 with a 1 GiB window is a memory decision, not only a parameter.
- **Access cost:** up to 11.26 MB decoded per read.

## 4. The floor — one stream

**measured (B1):** L = **17,695,928 B** for the same union = **21.018x**, as a single
zstd `-22 --ultra --long=30` stream in path-then-commit order. **composed:** x1.0242 + lean non-pack =
**~21,382,000 B apparent**.

````text
  +------------------------------------------------------------------+
  |  one zstd stream over 371,937,306 B                              |
  +------------------------------------------------------------------+
       a read of ANY version decodes the whole 372 MB
````

**This is a lower bound on the information in the corpus, not a design.** It is useful for
sanity-checking other numbers and for nothing else. Note that at 21,382,474 B the non-pack overhead
alone would be **15 %** of the Store — the fixed per-object cost stops being a detail at that scale.

**Also recorded:** `--long=31` is byte-identical to `--long=30` (B1, N2); a window at or above the
corpus size is the requirement, and no larger.

## 5. Claim ledger

| # | Claim | Label | Evidence |
| --- | --- | --- | --- |
| 1 | T1 keeps one record per pack group; T2 changes that | holds today | `cas/selection.rs:54-55` |
| 2 | Multi-record groups already exist for three lanes | holds today | `PackLane::Ordinary | Native | PooledMetadata`, `GroupCodec::Zstandard` |
| 3 | Grouping is 1.2637x overall but **2.95 % worse** on records that carry a dictionary | measured | B3, `squad-b/B3-framing-codec.md` |
| 4 | Grouping costs 59.8x single-record read amplification at 256 KiB | measured | B3 |
| 5 | T2 = ~44,374,000 B apparent, with one borrowed ratio | **est** | this paper §2 G3 |
| 6 | T3's content is 27,184,431 B at 254 frames, 13.682x | measured | B1, `squad-b/B1-floor.md` |
| 7 | T3 needs a path concept the Store does not have | holds today | `layerfs-storage/src/lib.rs:5-7`; schema |
| 8 | The floor is 17,695,928 B, 21.018x, no random access | measured | B1 |
| 9 | `--long=31` == `--long=30` byte-identically | measured | B1 N2 |
| 10 | A tier below v0.1.6 is reachable **at T1** | computed | `retained-history-t1/t1-implementation.md` |
| 11 | T2, T3 and the floor as **scheduled work** | **deferred** | below |

## 6. What would finalize this

1. **An access-cost ruling.** T2 costs 59.8x read amplification on the records it groups; T3 costs up to
   11.26 MB decoded per read. **No timing number exists for either, and none may be inferred from this
   campaign** — the machine was shared. The ruling needs a cold, uncontended, declared-cache
   measurement, per `AGENTS.md` §1.
2. **A re-measurement of T2's borrowed ratio** on the *post-T1* base-less population, rather than
   borrowing 1.1875x from a different size mix.
3. **A path-concept amendment**, before T3 can be designed at all.
4. **For the floor:** nothing. It is a bound; record it and stop.
5. **Then either promote the surviving tiers** to a description of shipped behaviour — in which case
   they move out of `deferred/` and into the set, with chapter numbers — **or delete this paper** and
   record the rejection here.

Until step 5 this paper stays a proposal, and any number read from it is an estimate.

## 7. Revisit condition

Re-open when **T1 has landed and its gate has been re-run under #186's contract**, *and* one of:

- a cold, uncontended access measurement shows grouped reads are affordable; or
- the Store gains a path or correspondence key for an independent reason, making T3's key free; or
- T1's realised size lands above the gate and the shortfall cannot be closed inside T1.

## 8. Related documents and upkeep

- [T1 — README](../../../../docs/roadmap/0.1/0.1.7/retained-history-t1/README.md) and
  [T1 — implementation specification](../../../../docs/roadmap/0.1/0.1.7/retained-history-t1/t1-implementation.md)
  — the tier under consideration.
- [Retained-history storage specification](../../../../docs/roadmap/0.1/0.1.7/retained-history-storage.md).
- [Deferred: delta hints across the size transition](01-size-transition-delta-hints.md) — the sibling
  deferral.
- Issues: [#189](https://github.com/Ephemeral-AI-Lab/layerfs/issues/189) (this paper's tracking
  issue), [#188](https://github.com/Ephemeral-AI-Lab/layerfs/issues/188) (T1 implementation, a
  sub-issue of [#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187)),
  [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186) (#187's parent),
  [#185](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185) (the sibling deferral).

**Upkeep.** If a source change makes a path key exist, makes grouped reads cheap, or moves any of these
tiers into `core/`, this paper changes in the same commit — or is deleted, with the rejection recorded.
Advancing the pin without a content change is allowed only when the change touches none of those, and
must be said rather than done silently.
