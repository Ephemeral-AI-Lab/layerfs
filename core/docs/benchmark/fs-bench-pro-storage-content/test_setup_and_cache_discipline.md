# Test setup and cache discipline

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Sibling documents: [`c1-families.md`](c1-families.md),
> [`c2-families.md`](c2-families.md),
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md),
> [`gates_and_oracles.md`](gates_and_oracles.md).
> Consumed by Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> and its two family issues ([#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182),
> [#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183)).
>
> **No figure in this document is a measurement.** Every number is a declared
> constant, a case configuration, or a citation to an existing receipt labelled
> diagnostic.

## 1. Two requirements that only appear to conflict

1. **Preparation must be fast and reusable.** Setup work is paid once and reused;
   repeating it before every sample is forbidden.
2. **A warm cache must never credit a measured phase.** The OS, the Store and the
   page cache are not part of the product's work.

The repository resolves the tension in one sentence:

> "reuse that removes work **outside** the timers is required; reuse that removes
> work **inside** a timed phase is cheating." — `AGENTS.md` §2

**The dividing line: reuse the bytes, never the residency.**

| Reusable | Not reusable |
| --- | --- |
| Generated or downloaded fixture bytes, persisted once | Pages left resident by setup, an earlier sample, or another arm |
| The prepared master, validated once per acquisition | The master's warmed page cache |
| A compatibility digest proving the entry is the right one | A "cold" label applied because a file is old |
| A per-sample independent **byte copy** | An APFS clone, a hard link or a reflink presented as a copy |

## 2. What must be prepared

An edit case does not need input bytes. It needs a **base state**: a file that
already exists, its canonical objects already built, and — for C2 — already **in a
Store**, because a delta trial can only read a base that is stored
(`measure_pooled.rs` records this: *"one save operation per leaf (a pooled base
must already be stored to be read for a trial)"*).

Three prepared artifacts:

| Artifact | Contents | Used by |
| --- | --- | --- |
| **Input bytes** | deterministic generator output | `c1.construct.*` |
| **Canonical object set** | the base file's finalized objects, as `<digest>/objects/<hex-id>` | `c1.edit.*`, `c1.transition.*`, C1-only filesystem cases |
| **Base Store** (master) | a real `.sqlite` with base objects saved and the watermark advanced | `c2.reuse.*`, `c2.delta.*`, `c2.read.*`, `c2.pool.*`, `pipeline.*` |

### 2.1 Why this is mandatory, not an optimisation

Take `c1.edit.length-changing.insert-middle-4k` on a 3.3 MB chunked base — the
shape of `edit_timing_c1`. The measured operation is one `apply_edits` over a
40,000-byte insertion. Building the base means: generate 3.3 MB, run CDC, build the
extent tree, emit roughly 200 canonical objects, and (in C2 mode) save them and
advance the watermark. That is tens of milliseconds against a sub-millisecond
measurement.

**Unprepared, the case measures setup.**

### 2.2 What happens inside the timed region, per case shape

This is the correction that governs the whole design:

| Case shape | Inside the timer |
| --- | --- |
| `c1.construct.*` | `construct_bytes` over a slice |
| `c1.edit.*` | `apply_edits` — base read through the provider |
| `c1.fs.*` | `update_filesystem` (+ `Store::open` in pipeline mode) |
| `c2.save.*` (**fresh run, no base**) | `Store::**create**` + save |
| `c2.reuse.*`, `c2.delta.*`, `c2.pool.*` | `Store::**open**` on a sample copy + save |
| `c2.read.waves` | `Store::open` on a sample copy + read wave |
| `pipeline.*` | `update_filesystem` + `Store::open` + save + ack |

**`Store::create` is inside the timer only for the fresh-run save family.** Every
edit, reuse, read and pool case **opens a prepared Store copy**. An earlier
statement in this document set implied otherwise; this table is the corrected
form. The case row carries the distinction explicitly:

```rust
store_state: StoreState::CreatedInSample | StoreState::OpenedFromCopy
```

Without that flag, two cases that both say "save" measure different work.

## 3. Preparing the master

```sh
runner.py prepare --case <id> [--seed N]
```

Build order (all untimed):

```text
generate or acquire bytes
  -> construct canonical objects
  -> Store::create + save + finish          (advances the publication watermark)
  -> validate: sha256 per file, PRAGMA quick_check == ok
  -> seal: chmod removes 0o222 (immutable master)
  -> write manifest.json
```

Lands at:

```text
benchmark-results/fs-bench-pro-storage-content/prepared/<compatibility-digest>/
  manifest.json     {compatibility, producer, created_ns, files{path -> {bytes, sha256}}, data_bytes}
  inputs/           fixture bytes
  objects/          canonical object set (C1-only cases)
  store.sqlite      base Store (C2 cases)
```

### 3.1 Reuse rules that apply verbatim

- The **master is validated once** per acquisition; its sealed digest is retained, so repeated source rehashing per sample is unnecessary.
- Entries must be **complete and quiescent** before publication; half-built entries and unexpected sidecars must never be consumed. `Store::open` already refuses a watermark ahead of storage (`Integrity`), which is a free integrity gate here.
- **Unknown compatibility fails closed.**
- A **selected run prepares only the selected case and tier.** *"Resolve the requested case, tier, seed and source arm before preparation. A selected run MUST NOT prepare all families or all four tiers."*
- The cache **must not** hold post-operation state, live Stores, measured-process state, or performance/verifier receipts. Expected-result data is verification-only and must never prime a mutation.

### 3.2 If fixtures are downloaded

Generators cover every current case, so a download is optional. If real corpora are
wanted, the acquisition is an **event with provenance**, not an untracked step:

- record URL, sha256 per file, total bytes, and date;
- seal the result with the same manifest and compatibility digest;
- report the acquisition wall separately as `preparation_wall_ns` — never folded into either the timed phase or the command total.

**Decided (owner R2/D2):** per-sample acquisition is reported as its own field
(`acquisition_wall_ns`), **outside** every operation timer and **outside** the row's
admission decision; the complete-command status is reported separately. v0.1.6's
18.57 s acquisition was excluded as "one-time validation" — that exclusion is *not*
precedent for omitting the number, so the `mincore`-first de-warm below is
**required**, and the campaign's real acquisition cost stays visible.

## 4. Per-sample acquisition

Each sample gets an **independent writable byte copy** of the master. The mechanism
already exists at `benchmark/fs-bench-pro/shared/runtime.py:537-568`
(`closed_store_copy`):

1. refuse a master carrying `-wal` / `-shm` / `-journal` sidecars;
2. `shutil.copyfileobj` with a 1 MiB buffer — **an independent byte copy, deliberately not an APFS clone**;
3. `flush` + `os.fsync`;
4. `chmod 0600`;
5. `PRAGMA quick_check` must return `ok`;
6. reject if `(st_dev, st_ino)` of master and copy match — *"sample aliases master inode"*;
7. record `clone_method: "closed-quiescent-byte-copy"`, `master_store_sha256`, `sample_store_sha256`, `store_bytes`, and both inodes.

After the sample, the master is **re-verified byte-for-byte** and `master_unchanged`
is recorded.

### 4.1 Where a copy is not required

Copying is for the **Store**, which the sample mutates. A read-only fixture needs no
copy at all: `mmap` the prepared file and hand the product a slice. That removes the
per-sample materialisation cost entirely — which matters because one process per
case means the fixture would otherwise be re-read every sample, and a 500 MB
materialisation is 0.3-1 s of pure setup repeated across the set. For construction
families the residency is declared anyway (`warm-in-process-fixture`), so no cold
claim is being lost.

### 4.2 The copy ladder (correction, review S2)

The single byte copy above is the **fallback**, not the only rung. `benchmark_rules.md:246`
permits *"a fresh independent writable copy **or real copy-on-write clone**"*, and
`clonefile(2)` is measured available on this host (`VOL_CAP_INT_CLONE` is set on the
volume holding the repository).

| Rung | Mechanism | Per-sample cost | Declared cache state | Legal when |
| --- | --- | --- | --- | --- |
| **R0** `read-only-master` | no copy; `mmap(PROT_READ, MAP_SHARED)` the master | de-warm only | `prepared-master-dewarmed` | the timed phase cannot write the artifact — all C1 construction/edit/read families, and every C2 **read** family |
| **R1** `apfs-clonefile-cow-v1` | `clonefileat` | single-digit ms, size-independent | `clone-fresh-vnode-unresident` (**never called cold**) | mutation cases, **and** `write_fraction_class != large`, **and** the case does **not** gate allocated bytes, **and** hardlink-free, **and** free-space preflight passes |
| **R2** `closed-quiescent-byte-copy` | `copyfileobj` + one target digest | ~1.1 s at 626 MB | `prepared-master-dewarmed` | default fallback; required wherever R1 is refused |
| **R3** `regenerated-in-process` | re-run the generator | 26.6 s measured for the 100k tree | `warm-in-process-fixture` | `setup: fresh` and no-master cases only — **not a speed rung** |

**Three constraints on R1, each rule-backed:**

1. **Never name it `--setup clone`.** `AGENTS.md:79` defines that token as *"a byte
   copy — not an APFS clone"*, and historical receipts stay comparable only if the
   meaning is stable. Add `--setup reflink` and a default `--setup auto` that
   resolves, records the rung and its reason, and must resolve **identically for both
   arms** or the pair is refused.
2. **A clone's `st_blocks` double-counts blocks shared with the master**, so R1 is
   **forbidden for `c2.footprint`** and for any row gating `store_allocated_bytes`.
   The repo states this twice: *"Copies/APFS clones are not allocation controls"*
   (`0.1.4/issue88-delivery/contract-v1.md:174`) and *"APFS clones/copies preserve
   content, not allocation equivalence"* (`0.1.4/issue87-analysis:138`). Every row
   carries `allocation_attribution: exclusive | shared-with-master`.
3. **ENOSPC is a real hazard R2 does not have.** `clonefile(2)`: *"it is possible for
   a subsequent overwrite of an existing data block to return ENOSPC."* Require
   `free_bytes >= 1.05 x master_logical_bytes` before choosing R1.

`copyfile(3)` is **not** an acceptable route: `COPYFILE_CLONE` silently falls back to
a byte copy (forbidden by the no-silent-fallback stance), and `COPYFILE_CLONE_FORCE`
refuses directories. Use `clonefileat` per entry, or R2.

**Delete three per-sample full-content passes.** The path this section previously
lifted from `runtime.py:537-568` reads and SHA-256s the source **twice** and the
target once, runs `PRAGMA quick_check` per sample, and re-hashes the master after the
sample — four full passes where `benchmark_rules.md:251-252` says *"repeated source
rehashing per sample is unnecessary."* At the measured 1.25 GB/s validation rate that
is **~2-3 s of setup for one 626 MB sample**. Replace with: one bounded identity
sample (8 deterministic pages), master validation **once per acquisition**, and
`master_unchanged` by stat-identity — with the master path never released to a sample
process, recorded as `master_path_released_to_sample: false`.

## 5. De-warming

A copy is warm the moment it is written. Before the clock starts, the sample's copy
is invalidated and verified:

```text
open(O_RDONLY) -> mmap(PROT_READ, MAP_SHARED)
  -> mincore(addr, len, vec)              -> resident_first   # does NOT fault pages in
  -> if resident_first > 0:
         msync(addr, len, MS_INVALIDATE)  # no MS_SYNC needed: the copy is clean
     mincore(addr, len, vec)              -> resident_pages
  -> require resident_pages == 0 and pages_checked == expected_pages
```

**Correction (review S2): `mincore` first, not touch-every-page.** `cold.py:52-54`
touches every page before invalidating (*"Instantiate shared mappings before
invalidation"*), which is why the v0.1.6 campaign paid a measured **18.57 s** of
acquisition for a 100k-file fixture. A non-resident page needs no invalidation —
`mincore` already proves it. Recording `resident_first != 0` is **not a failure**; it
is the evidence that the de-warm did work, and the honest way to show the instrument
is live. The measured per-file cost falls from ~186 microseconds to roughly the cost
of `open`+`mmap`+`mincore`+`munmap`+`close`.

The primitive is implemented and **self-tested** at
`benchmark/fs-bench-pro/shared/cold.py:28-80`; `self_check()` (`:65-80`) proves it
both detects *and* evicts known-warm pages. Its acquisition path (`:113-183`) also
records `pages_checked`, `expected_pages`, `allocated_bytes = st_blocks * 512`, and
`fixture_digest`.

**The residency gate is fail-closed:** `resident_pages != 0` or
`pages_checked != expected_pages` makes the row `INELIGIBLE` — never quietly fast.

After de-warming, a measured base read pays a **real storage read**. That is the
honest price: the #151 ledger's **2.1 GiB/s from storage**, not the **19 GB/s**
cache-credited figure for the same bytes.

**Cost:** `msync` is O(file size) and `mincore` is page-count linear. Negligible at
MB scale; at 500 MB both are real work. **De-warm only the cases whose timed phase
actually reads the file** — running it on a construction case that never opens the
artifact is pure waste and adds nothing.

## 6. Cache-state taxonomy

Every case declares exactly one state, and **states are never pooled**:

| State | Meaning | De-warmed? |
| --- | --- | --- |
| `warm-in-process-fixture` | bytes read into memory during setup; the timed phase performs no file I/O | n/a |
| `prepared-master-dewarmed` | copy made, then `msync(MS_INVALIDATE)` + `mincore` verified zero | **yes** |
| `prepared-master-warm` | copy made, de-warm not applied | no — quarantine |
| `product-warm` | the timed phase reads packs it wrote in the same sample | impossible (§7) |
| `verified-cold` | full cold contract: invalidation + residency + digest + metadata (§6.1) | yes |
| `clone-fresh-vnode-unresident` | R1 clone; residency verified on the clone's own vnode | yes — **but see the E2 caveat below** |
| `uncontrolled` | none of the above; declared explicitly and never admissible | no |

**Unproven pending experiment E2 (review S2).** A clone is a distinct *vnode* whose
extents are shared with the master. The claim that its residency is therefore
independent of the master's — and that `msync(MS_INVALIDATE)` on the clone is
vnode-scoped and cannot evict the master — is **inference from XNU's UBC design, not
documentation**. `man 2 msync` says only *"Invalidate all cached data"*, without
saying whose. **If the cache were keyed by physical extent, a clone would inherit the
master's residency and `prepared-master-dewarmed` would be a lie for R1.** Until E2
passes, an R1 row claiming a de-warmed or cold state is reported `INELIGIBLE`, not
`PASS` — or R1 is restricted to cases that make no such claim.

E2 is cheap and untimed (~2 s): clone a master; `mincore` the clone (expect 0); read
the master fully (expect its pages resident); `mincore` the clone again — **expect 0,
and this is the decisive observation**; `msync(MS_INVALIDATE)` the clone; `mincore`
the master and record whether it dropped. Repeat on a clean and on a freshly-written
master, because the clone-of-dirty-master path is the one preparation actually
produces.

### 6.1 The cold contract, if it is used

`cold.py` currently applies to exactly one case in the whole v0.1.6 harness
(`applies()` returns true only for `init_namespace / namespace-100000`) and
hard-codes `CONTRACT`, `METHOD`, `TARGET_NS = 2.7 s`, `FIXTURE_DIGEST` and a fixed
mtime in its contract block (`:13-19`), and checks the fixture sizes — 100,000 files,
1,001 directories, 500,000,000 bytes — against the receipt at `:134`, `:175`,
`:200-201` and `:226-228`. Reusing it for a core case means **parameterizing those constants and
stamping a new `contract`/`method` pair** — not editing the existing one, and never
relabelling an existing receipt.

Its `assess()` (`:186-251`) is the model to copy: it **recomputes** eligibility
from raw evidence rather than trusting a stored boolean, and returns
`eligible_elapsed_ns = None` when ineligible.

## 7. The case where de-warming is impossible

If the measured operation reads packs **it wrote in the same sample** —
`c2.read.waves`, and the readback half of `pipeline.*` — de-warming would change
what is being measured. There:

- declare `cache_state: "product-warm"` with a short reason;
- **quarantine** those rows: never summarize them beside a cold or de-warmed read;
- never describe them as cold, and never derive a cold-read claim from them.

This is the same resolution v0.1.6 reached: declare the state, and refuse to pool.

## 8. Enforcement — four gates

1. **Residency gate.** Any row claiming `verified-cold` or `prepared-master-dewarmed` must show `resident_pages == 0` and `pages_checked == expected_pages`.
2. **Distinct setup identity.** `prepared-master-copy` and `fresh-input` carry different `setup_identity` values and are never pooled. This is what stops "reused" from silently becoming "fast".
3. **Cache-leak diagnostic, sampled.** Run **one representative read-bearing case per family** twice back-to-back — not all cases; doubling every case's cost to catch a leak is waste. It is a **labelled diagnostic whose purpose is to refuse warm credit**: if the second run is materially faster, the case has a cache leak and the row is not admissible. Diagnostic only — the first run stays the gate sample; this is not a best-of selection. The precedent is stark: L19 recorded a control create at **8.02 s cold vs 0.90 s back-to-back — a 4.7x artifact**.
4. **Both arms equal.** If one arm is prepared and the other fresh, or one is invalidated and the other is not, the pair is void. Cache state is declared and enforced *equally*.

## 9. Receipt fields

```text
setup_identity            prepared-master-copy | fresh-input
compatibility_digest      key the master was accepted under
prepared_root             where the master lives
copy_method               closed-quiescent-byte-copy | regenerated-in-process | not-applicable
master_unchanged          <bool>, re-verified after the sample
store_state               created-in-sample | opened-from-copy
cache_state               one of §6
resident_pages            required == 0 when the timed phase reads the file
pages_checked             must equal expected_pages
preparation_wall_ns       setup cost, separate field
command_wall_ns           the whole invocation
```

## 10. Budget accounting

| Work | Frequency | Rough cost |
| --- | --- | --- |
| Generate or acquire fixture bytes | once per compatibility digest | ~0.5 s for 500 MB (xorshift), or a download |
| Validate the master | once per acquisition | linear in fixture size |
| Copy the master Store | per sample | ~ms at MB scale; ~0.3-1 s at 500 MB |
| `mincore`-first de-warm | per sample | page-count linear but non-faulting; `msync` runs only if `resident_first > 0` |
| Copy, R1 `clonefileat` | per sample | single-digit ms, size-independent |
| Copy, R2 byte copy | per sample | ~1.1 s at 626 MB |
| **Per-sample full-content passes** (source re-hash, `quick_check`, master re-hash) | per sample | **0 — deleted**; replaced by master validation once per acquisition |
| Materialise into the sample process | per sample | untimed read |

Two things follow. First, **persistence eliminates generation and acquisition, not
materialisation** — one process per case means each sample still reads its bytes,
and that read is setup. Second, the **≤ 15 s complete-command budget** covers the
whole invocation: product timer plus lifecycle plus cleanup. Phase 0's entire set
ran at 0.004-1.7 s per command, so the budget is comfortable at these sizes; a
500 MB copy is the one step worth watching, and it is declared rather than hidden.

## 11. Cleanup and retention

- **Fresh `--output` per run**; receipts are append-only and are never overwritten. Every example already refuses an existing `--output` (`measure_pooled.rs:76`, `measure_edits.rs:160-167`, `measure_components.rs:97-104`).
- **The sample Store is disposable**, because cleanup mutates the artifact: `cleanup.rs:65,120` issue paged `DELETE`s. This is why **arms can never share a Store**.
- **Sample cleanup must not delete the master.**
- **The measurement lock is held per `perf`/`verify` invocation**, not across a family run, so preparation can proceed independently while measurement stays exclusive — within the worktree that owns the lock (owner direction, 2026-09-21 — [isolation](../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)); a run in another worktree is not excluded.
- **Failed and discarded attempts stay on disk** with their exit codes.
- Deferred packs of a failed save stay unreadable by design (the watermark does not advance), so a discarded sample proves nothing about durability and must not be cited as if it did.

## 12. Non-goals

- No warm-cache credit of any kind; no cold claim without the §6.1 contract.
- No pooling of cold, de-warmed, product-warm or uncontrolled rows.
- No hard links to a master; no APFS clone presented as a byte copy.
- No preparation inside a timed phase; no repeated setup before a sample.
- No retention of post-operation state, live Stores or verifier receipts in the cache.
- No Docker, container or cgroup involvement anywhere in preparation or acquisition.
