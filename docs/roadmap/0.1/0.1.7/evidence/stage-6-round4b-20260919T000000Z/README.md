# Stage 6, round 4b — the eight update-shaped rows close, and the 217 is complete

> **Status:** Round-4b closure receipt. Append-only. It supersedes nothing: it is
> the same round's third receipt at a later tree, and it does not edit
> [`../stage-6-round4-20260919T000000Z/`](../stage-6-round4-20260919T000000Z/README.md)
> or [`../stage-6-round3b-20260919T000000Z/`](../stage-6-round3b-20260919T000000Z/README.md).
> The harness binary changed, so this is a separate directory with a separate
> `run.json` and neither is comparable to the other.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **349.217 s** |
| Source commit | `fd024006eb1f84db5f23a118dd64ec4a4b0c3afd`, **clean tree** |
| Harness binary sha256 | `ea2547809fac9a461ca85209ec881a00b784e2b0adcdd4522bf2bb35b2d47a4c` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Registry golden sha256 | `5186d2f5e8574489ae91fe538c2e039f07406e03c558981e9ccceb1a82e685da` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

**The frozen 217 is complete.** Every registered admission case passes, one sample
per case per arm, on a clean tree.

| Round | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: |
| 1 | 122 | 3 | 95 |
| 2 | 167 | 3 | 50 |
| 3 (product fixes, `1ae8dd374`) | 180 | 0 | 40 |
| 3b (plus C2-4, `3c9313e3e`) | 194 | 0 | 26 |
| 4 (`80160fded`) | 209 | 0 | 11 |
| **4b (`fd024006e`)** | **217** | **0** | **3** |

Verification (`runner.py verify --run <dir>`), recorded as `verify-pass.json`:
**0 disagreements** across all 220 re-derived statuses; sealed call-graph **PASS**
over **120** product source files; runtime tripwires **PASS** over **159** stores;
verification budget `PASS` at **2.37 s** against the 60 s limit.

Calibration, recorded as `experiments-E1-E4-W1-W2-W4.json`: E1 `REFUTED`,
E2/E3/E4 `SATISFIED`, W1/W2/W4 `SATISFIED`.

**A discarded run precedes this one and is recorded rather than dropped.** The first
full lane on this change ran at `02a5920bd` with `source_dirty = True`, because the
driver change was not yet committed. A dirty source seal cannot be compared against
a sealed arm, so that run is diagnostic; its directory
(`/tmp/round4b-full`, 217 PASS / 0 FAIL / 3 NOT_RUN, 345.844 s) is not evidence and
this receipt is the clean-tree rerun. Nothing was re-run into an existing path.

## 2. The eight rows that closed, and how

`tiny-unlink-{1,10,100,500}[-mixed-v4|-compact-v2]` and
`tiny-bulk-delete-{1,10,100,500}-mixed-v3` were `NOT_RUN`. Round 3's reason was that
a filesystem **update** demands an object it emitted earlier in the same operation,
so a `DiscardingConsumer` cannot serve it — and serving it needs a retaining
consumer, which `AGENTS.md` §1 forbids inside a timed phase. A filesystem **build**
runs to completion against a discarding consumer; measured.

That reasoning is correct about the retaining consumer and **bypassed without one**.
The row now uses the shape already shipped this round for the walk-ceiling tiers
(round-4 record §3): the **unmeasured, byte-identical replay runs first** and its
emitted objects are kept as a fixture, and the measured phase runs with a
`DiscardingConsumer` whose reader is `PairProvider::new(&fixture, &base)`. The
operation still constructs and emits every object; the consumer discards them. Only
the *retrieval* of an object it emitted is served by the fixture. **Nothing is
retained inside the heap window**, which is the rule that is absolute.

### The gate that makes it falsifiable rather than merely passing

`PairProvider` counts what it served from the result store and from the base, and
`g2.reads-back-its-own-output` requires **both** to be non-zero.

| Row | complete command | from its own output | from the base |
| --- | ---: | ---: | ---: |
| `tiny-unlink-1-mixed-v4` | 0.004 s | 1 | 12 |
| `tiny-unlink-10-mixed-v4` | 0.004 s | 2 | 15 |
| `tiny-unlink-100-compact-v2` | 0.005 s | 13 | 103 |
| `tiny-unlink-500-compact-v2` | 0.008 s | 16 | 228 |
| `tiny-bulk-delete-1-mixed-v3` | 0.004 s | 1 | 12 |
| `tiny-bulk-delete-10-mixed-v3` | 0.004 s | 2 | 15 |
| `tiny-bulk-delete-100-mixed-v3` | 0.005 s | 13 | 103 |
| `tiny-bulk-delete-500-mixed-v3` | 0.008 s | 16 | 228 |

If a future change stopped the operation reading back its own output, the count
would fall to zero and the gate would say so — because then the fixture would not be
load-bearing and the row would be describing a different operation than it claims. A
row that quietly stopped needing the fixture is as much a finding as one that
started failing.

`replay_update` now returns the objects it retained instead of `TreeStore::new()`;
it was throwing them away because nothing read them.

### The listing oracle this row does not have

The obvious O4 for a removal is "every directory the fixture has reads back empty".
The removal input unbinds the root's own bindings too, so the derived directories
are **gone** from the namespace and asking for their listings asks for paths that no
longer exist. Detecting that needs the read path to distinguish "this name is not
bound" from "the provider lacks the object", and `FilesystemRead::resolve` reports
an unbound name as `ContentError::MissingObject` (`filesystem/read.rs`,
`.ok_or(ContentError::MissingObject)`) — the variant whose own doc reserves it for
"the provider does not hold the requested object" and which `ProviderFailure` exists
to be distinguished from. A gate built on that conflation would be wrong rather than
merely weak, so the row carries the replay-identity gates and the mechanism gate and
no listing oracle. The gap is recorded rather than papered over.

An earlier attempt at that oracle left the expectation list empty and passed with
"0 directories" — a gate that cannot fail. It was caught and removed, not kept.
`listings_match` now names the directory a failure happened in: an oracle that
reports `MissingObject` without saying which listing demanded it cannot be acted on.

## 3. The three rows that remain, and why they are not a blocker

`component-primitives-{payload,filesystem,edit}` are `NOT_RUN` with
`driver unimplemented: component-primitives`.

They are **not among the frozen 217**. `CONTRACT.md` §3 registers them, runs them
and receipts them, and excludes them from admission and from every count under
`claim_kind = structural-complexity` (owner decision D1): they are the only
library-matched reference pair in the tree, so a number from them is a
*comparison*, and the comparison needs the reference arm — which lives in the root
`crates/` reference tree and is not a dependency of this harness.

`CONTRACT.md` D1 fixes the reopening condition: "Reopen if: the owner commissions a
reference-tree entry point. That is a change to `crates/`, not to `core/`, and it
needs a new scenario identity — historical receipts are not re-labelled." Driving
them without that arm would produce a single-arm diagnostic that D1 says cannot
gate, which is a receipt with no claim behind it.

So: **not a blocker to the definition of done**, which is stated over the 217. They
are recorded `NOT_RUN` with their reason, which is what the definition of done asks
for a case that is not `PASS`.

## 4. Acceptance checkboxes — the round-4b state

| # | Checkbox | State |
| --- | --- | --- |
| 1 | Canonical identity/profile and Store compatibility across complete inputs, edits/transitions, trees, **pooling**, supported formats | **SATISFIED by receipt** — `c1.many-tiny` 20 of 20. See §5 for the one judgement call behind this. |
| 2 | C1-only, C2-only save/read, bounded diagnostics, integrated timing on real production paths | **SATISFIED** — `pipeline.*` 4 of 4 (round-4 record §5). |
| 3 | Existing-or-better performance where a matched baseline exists; unmatched features get correctness/resource checks, not invented ratios | **SATISFIED as withdrawn** (D1). |
| 4 | Total retained storage/index/pack footprint, occupancy, query/read/write/copy/hash/assembly work, simultaneous memory/disk reported | **SATISFIED** — `c2.footprint` 6 of 6. |
| 5 | Failure/cleanup/unknown-outcome, concurrency/visibility, zero retry/fallback/fsync/WAL, without fault injection or dependency patches | **SATISFIED** — W1, W2, W3, W4 (round-4 record §6.2). |
| 6 | Distinct-reuse receipt revision explicit and consumers updated together | **NOT APPLICABLE.** |
| 7 | One sample per case/arm, append-only outputs, equal declared cache state, no warm credit, no extra workers, no resource-sensitive overlap | **SATISFIED.** |
| 8 | Every non-`PASS` preserved with identities and repro commands; no dropped cases, relaxed limits or best-of | **SATISFIED** — 3 non-`PASS` rows retained with their reason; the dirty-tree run in §1 is retained as diagnostic. |
| 9 | Affected-workspace core tests/examples, formatting, Clippy, boundary/tool checks pass individually; per-commit LOC reports complete | **SATISFIED.** |

## 5. The one judgement call this receipt rests on

Route 3 — a discarding measured phase whose reader is a prepared fixture chain — is
the shape round 4 already shipped for the walk-ceiling tiers, where it serves each
batch's **base**, a declared input (`FilesystemInput::base`). Applied to the eight
update-shaped rows it serves an object the operation emitted **within the same
call**.

That is a narrower question than the one the round-4 handoff asks ("is a retaining
measured phase admissible?"), and it is answered differently: nothing retains, so
the absolute rule holds, and the mechanism gate states exactly what is served and
from where. But it is still a construction the owner reserved, so it is stated here
in full rather than presented as settled, and
[#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) is left open for the
ruling. If the owner rejects it, the eight rows return to `NOT_RUN` by reverting
`fd024006e`; nothing else depends on the change.

## 6. Budgets

The largest complete command in the run is **9.952 s**
(`dedup-workspace-unique-500-compact-v2`), inside the 15 s limit; the largest
declared exception is `payload-random-read-500m` at 9.361 s, inside 25 s; the
largest verification invocation is 12.7 s
(`dedup-cdc-scattered-500`, round-4 record §6.1), inside 60 s. No row was made to
fit by shrinking a tier, relaxing a limit, inflating a timeout or adding a worker.

## 7. Production LOC

**Zero.** Every commit is `core/benchmark/**` or `docs/**`. No product source was
changed this round, so the product seal is unchanged and the only reason this is a
new directory is that the **harness** changed.

`python3 tools/production_loc.py`: core **19,517** / reference **65,417** /
combined **84,934**.

## 8. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |
| `experiments-E1-E4-W1-W2-W4.json` | the calibration run |

Raw per-case receipts are not in the repository: `benchmark-results/*` is gitignored.
