# The declared cache stance for `history.*`: what is true, what is undeclared, and what would verify it

> Status: Research; a written answer, not a contract, and not a cold claim. Written
> before building anything, per the round's instruction. Every number is a
> re-derivation over the retained campaign's existing receipts and traces; no
> measurement command was run.

## 1. The question, disambiguated

The handoff asks "whether `CreatedInSample` is the correct declaration for this
shape and whether it can be *verified* — residency plus `disk_read_bytes` measured
at state boundaries and asserted, rather than assumed".

That question is answerable only after splitting the row's inputs into two classes
with opposite answers, because `CacheState` is a single field and the row has two
kinds of pages serving it:

| Axis | What it is | Declared today? |
|---|---|---|
| **endogenous** | the Store's own bytes, written by the operation inside the timed region and read back by later states of the same operation | yes — `CacheState::CreatedInSample`, `StoreState::CreatedInSample` |
| **exogenous** | the deepseek-history corpus, an immutable directory outside the output path, read by the harness between the measured children | **no** — nothing declares, measures or enforces its residency |

The row is `INELIGIBLE` because of the second row of that table, not the first.
Nothing about the Store can be fixed, verified or re-declared to change it.

## 2. The endogenous axis: `CreatedInSample` is correct, and residency is the wrong instrument

`src/registry.rs:53-59` defines the state as "Base written by the operation itself
inside the timed region", and `src/families/history.rs:37-38` declares it, with
`Preparation::InProcess` and no master, copy or checkpoint
(`src/ops/history.rs:17-21`).

Three consequences, in order:

1. **`resident_pages == 0` is false by construction here.** The Store is written
   inside the timed region; between state *k* and state *k+1* its pages are
   resident because the operation just wrote them. Running
   `gates::residency_gate` (`src/gates.rs:417-434`, which requires
   `resident_pages == 0` "wherever the row claims de-warmed") against this axis
   would not verify the claim — it would contradict it, and de-warming between
   states would invalidate the operation's own work microseconds after it was
   written, charging the operation for re-reading itself. That is the distortion
   the handoff already identified, and it is disqualifying.
2. **`disk_read_bytes` is the wrong instrument for the same reason.** Its gate is
   defined only for cold or de-warmed reads: "a row claiming a cold or de-warmed
   read must show `disk_read_bytes >= 0.9 × requested`" (`src/gates.rs:380-387`).
   For a Store created in-sample, demanding device reads would demand that the OS
   *not* serve the operation's own recent writes from cache — i.e. it would
   penalise the correct behaviour. (Separately: `gates::device_attestation` has
   **no call site anywhere in the tree**, and no operation records the instrument,
   so this is a rule about an unused primitive, not a rule being applied.)
3. **What is verifiable is provenance, not residency.** The assertable invariant is
   "every byte the read path returns from the Store was written by this same
   operation, inside this same timed region". Its observable fingerprint is
   already in the retained traces, measured by
   [state-provenance.txt](state-provenance.txt):

   | run | states | states whose read counters are all zero | first state returning canonical bytes |
   |---|---:|---|---:|
   | candidate2 stride10 | 17 | `[1]` | 2 |
   | candidate2 stride3 | 53 | `[1]` | 2 |
   | baseline2 stride10 / stride3 | 17 / 53 | `[1]` / `[1]` | 2 / 2 |

   All eight published per-state read counters (`filesystem.provider.*`,
   `filesystem.validation.*`, `filesystem.references.*`) are zero at state 1 and
   only at state 1; the cumulative returned canonical bytes are byte-identical
   between the arms (148,826,516 / 377,985,893), which is what a treatment with
   byte-identical Stores should produce. State 1 builds a new filesystem
   (`base: None`, `ops/history.rs:24-27`), so there is nothing to read; every later
   state reads the root the operation wrote one state earlier, through the Store
   the chain is growing.

   That fingerprint is evidence *for* the declaration. It is not a proof, and it
   cannot distinguish a page served from the page cache from one served from the
   device — which is exactly the distinction `AGENTS.md` §1 makes, and the reason
   this axis needs no cold claim in the first place.

**Disposition, endogenous axis: keep `CreatedInSample`. Do not add a residency
gate and do not add a device-attestation gate. If the row later needs an enforced
form of this claim, the enforceable pieces are (a) the Store path did not exist
before the timed region — already enforced at the producer, since the child refuses
an existing `--out` and the runner refuses an existing case directory
(`runner.py:888-890`) — and (b) the state-1-silent-read invariant above, which
could be asserted as a driver gate over counters it already publishes.**

## 3. The exogenous axis: the actual gap

The corpus is not prepared, not copied and not checkpointed
(`src/ops/history.rs:17-21`: "there is consequently nothing to prepare and nothing
to reuse … a second run costs what the first cost"). It therefore cannot be served
by the `--setup clone` / prepared-master route, and no cache contract covers it.

What that means concretely:

* The corpus pages are read **inside the invocation but outside every measured
  child** (`src/ops/history.rs:1-15`), and the harness publishes exactly that:
  `history.corpus_read_ns` (resource kind, basis "the harness's own corpus reading:
  inside the root, between the children, untimed"), measured at 16,161,397,833 ns
  (candidate2 stride10) and 22,943,708,667 ns (candidate2 stride3).
* Whether those pages were resident when the last run left them is unmeasured and
  uncontrolled. The retained campaign ran the same case four times on one machine
  against one immutable corpus directory; the receipts declare the gap honestly
  (`"cache_contract": "fresh-growing-store; OS/intra-chain residency
  uncontrolled"`), and that declaration is the reason `admission_eligible` is
  false.
* The corpus is **immutable and identity-pinned**: manifest SHA256
  `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, verified
  again in [custody.json](custody.json), tip `b0a7d2ce…`, with the row's own
  `prepare` publishing `history.manifest_sha256` and `history.source_tip`
  (`src/ops/history.rs:1394-1440`). So corpus residency is a *performance*
  question about a frozen input, never a correctness question about which bytes
  were read.

### The decision this leaves

**Is "immutable, identity-pinned corpus; residency uncontrolled and unenforced;
read wholly inside the timed region but outside every measured child" an
acceptable declared cache stance for an admission row?**

* **No** → the rows stay `INELIGIBLE` by declaration, permanently, and no
  engineering changes that. This is the status quo, and it is coherent.
* **Yes** → the stance must be *declared and measured*, not assumed. The minimum
  honest build is: publish the corpus's residency at operation start and end
  (`shared/residency.py`'s `mincore` method, mirrored in
  `src/support/instruments.rs`) and the operation's `disk_read_bytes`, as
  diagnostics, in the receipt's `cache_contract` block — without de-warming, without
  changing what the timed phase does, and without calling any of it a cold claim.
  That converts "uncontrolled" into "measured and declared". It does **not**
  by itself produce an admission row: B1 (admission declaration), B2 (budget class)
  and B3 (phase reconciliation) all still have to be settled, and the rows would
  remain `INELIGIBLE` until the owner rules on the sentence above.
* **Cold, instead** → the corpus would need its own cold contract (invalidate
  before the run, assert zero residency, charge the device reads inside the timed
  region). That is a new contract for a multi-gigabyte corpus, it is
  budget-affecting, and the reference harness's `shared/cold.py` covers exactly one
  case (`init_namespace`/`namespace-100000`) — so it is a bigger owner decision
  than the one above, and it is the wrong instrument if the owner's ruling is that
  a frozen input's residency is not a correctness matter.

## 4. What this answer changes

* The handoff's framing — "whether `CreatedInSample` is the correct declaration and
  whether it can be verified by residency plus device reads" — is answered **no**
  for the verification method and **yes** for the declaration, and the gap moves to
  the corpus axis, where it belongs. A cold claim cannot be manufactured here, and
  none is proposed.
* The row's `CacheState` field is not wrong; it is **incomplete as a description of
  the row**, because a single field carries the Store's state and says nothing
  about the corpus. Any admission claim for `history.*` must name the corpus
  stance in words (as the receipts already do) or carry a second field.
* Nothing in this document authorises a harness change. The diagnostic build in §3
  is specified, not implemented, and it is budget-affecting only in the sense that
  it adds a measurement to a command that already does not fit (DISPOSITION §2.4).
