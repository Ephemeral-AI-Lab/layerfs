# Verifying a retained-history row

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §6
> and §11, and by [`../gates_and_oracles.md`](../gates_and_oracles.md).

Verification is a **second, unmeasured invocation** with its own 60 s budget. It never
enters a performance distribution, and a verifier failure preserves the performance
sample while failing the row's admission.

## 1. The oracle set for a history row

The four oracles this campaign uses, and the honest cost of each:

| oracle | what it checks here | coverage | cost |
| --- | --- | --- | --- |
| **O1 identity** | the state's filesystem root `ObjectId` equals a pinned constant | full | O(1) |
| **O2 logical equality** | each file's bytes read back **through the Store** and compared to the corpus oracle's sha256 | **deterministic 10 % by default** | proportional to file bytes and chain depth |
| **O3 structural count** | canonical bytes, object count, pack count pinned as constants | full | O(1) |
| **O4 tree equality** | the state's entry manifest — path, mode, size, plus directory entries — equals `oracles/<sha>.json` | full | metadata only; cheap |
| **O6 footprint** | allocated vs apparent, `pack_bodies <= database`, freelist, sidecar absence, and the attribution split | full | O(1) SQL and `stat` |

O1, O3, O4 and O6 are **never sampled**. They are O(1) or metadata-cheap, and they carry
the claim. Only O2 is sampled, because O2 is the only oracle whose cost scales with the
history: v0.1.6's verification wall was 199.8 s on 53 states and 570.6 s on 157, against
306,861 and 904,143 path-states.

## 2. The deterministic sample

The rule is the harness's existing one, stated once and applied wherever a row declares
a countable unit:

```text
max(1, ceil(n/10)) units, selected by index % 10 == 0 in declaration order
```

**Declared unit: the state's file manifest** — the same list O4 compares — in corpus
order. Not the object count, not the pack count, and never "the first ten".

**Endpoints are included.** This campaign requires the first and last manifest entry in
every sample, because that is where boundary defects live. The current implementation
(`src/ops/mod.rs` `sampled_indices`) takes every tenth index with `take(ceil(n/10))`,
which for 53 units yields indices 0, 10, 20, 30, 40, 50 and **omits the last**. That is
owner decision 6 in the [README](README.md#6-owner-decisions-still-open); until it is
ruled, a sample under this campaign is not admission evidence.

The selection rule is named in the receipt, so a sample is reproducible from the receipt
alone. Every receipt carries `verification_mode`, `verification_declared_units`,
`verification_sampled_units`, `verification_selection`, `verification_omitted` and, when
a proof was reused, `reused_proof_identities`.

## 3. What we validate against

Five independent sources, strongest first:

| # | source | validates | coverage |
| --: | --- | --- | --- |
| 1 | `oracles/<sha>.json` — every path's mode, size and sha256, plus directory entries | the state's tree and its bytes | O4 full · O2 sampled |
| 2 | `inputs/<sha>.receipt.json` → `blob_digests` (oid → sha256) | every blob the reader serves | full, fail-closed |
| 3 | `manifest.tsv` / `previous.tsv` with the per-state `manifest_sha256` | the fixture identity, before any object is built | full |
| 4 | v0.1.6's recorded canonical totals on byte-identical trees (§3(b)) | the **product's encoding**, not just the fixture | lane-level gate |
| 5 | the Store's own invariants — schema identity, 4 tables, 2 indexes, watermark `I1`, `quick_check == ok`, sidecar absence, `pack_bodies <= database`, `freelist_count`, `page_count` | the Store is well-formed | full |

Plus the harness's own self-validation: phase reconciliation
(`prep + op + verify + cleanup ≤ invocation ≤ wall`), residency
(`resident_pages == 0`, `pages_checked == expected_pages`), `disk_read_bytes`, and window
containment and sibling non-overlap.

**What we deliberately do not validate against.** A replay that merely equals the measured
result — that is self-consistency, and a replay that faithfully reproduced a *wrong*
operation would satisfy it; O1 here compares against a **pinned** constant instead. The
v0.1.6 Store allocated bytes — a different operation surface, and a labelled reference
point only. And Git — cited constants, never re-run.

### 3.1 Where the pins come from

Three kinds of pin, and they are not equally strong. Saying which is which matters.

**(a) Corpus pins — available immediately, fully independent.** Path-states and logical
bytes are properties of the corpus, checked against it directly:

| lane | verified path-states | verified logical bytes |
| --- | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 |
| `history-stride3` | 306,861 | 1,676,767,835 |
| `history-stride1` | 904,143 | 4,936,693,030 |

**(b) Cross-generation canonical pins — available immediately, and the strongest check
this campaign has.** Recorded from v0.1.6 on byte-identical source trees, and preserved
by v0.1.7's canonical-identity policy:

| lane | canonical content | canonical objects |
| --- | --: | --: |
| `history-stride3` | 589,423,458 B | 73,476 |
| `history-stride1` | 871,588,115 B | 104,705 |

Reproducing these proves the migration is faithful. Not reproducing them is a **finding,
not a fixture tweak** — the stop rule in `implementation-plan.md` §4.

**(c) First-run pins — legitimate, and only because of what happens next.** The per-state
root identities of (a)/(b) do not exist yet, and recomputing them "off the product path"
is not independent. So the first full lane establishes them, and thereafter the row must
**reproduce** them, with a counter that moves being a `FAIL` rather than a new baseline.
This is exactly the mechanism `src/workload/expected.rs` already uses for the 217, whose
constants were taken from the last all-`PASS` run and compiled into the binary with
`include_str!` so the harness identity covers them. The same applies here: the history
table is embedded, not read at runtime.

## 4. The mode ladder

| mode | oracle | status it can produce |
| --- | --- | --- |
| `full` | every oracle, O2 over the whole manifest | `PASS` |
| `sample` | O1/O3/O4/O6 full, O2 over the deterministic 10 % | **owner decision 1**: `PASS` for this campaign only, `INCOMPLETE` elsewhere |
| `none` | nothing | `INCOMPLETE`, never `PASS` |
| `reused` (`--reuse-pass`) | one identity-matched `status=PASS` receipt | `PASS` |

**`sample` is the proposed default for these lanes**, and the reason it can be is that
the storage claim is not sampled: the O(1) counters that decide storage are read in full
in every mode. The 217-row contract keeps `full` as its default, because flipping it
there is a change after collection and would let a 10 % oracle admit rows whose frozen
oracle is O1+O2.

`--reuse-pass` accepts one identity-matched receipt instead of re-running verification. It
fails closed on schema, identity, hard-limit and wall mismatch, and records
`reused_proof_identities` plus an explicit omission. `--skip-verification` exists for
iteration and marks every row it touches `INCOMPLETE`, never `PASS`.

## 5. What is not required, and what is

The frozen oracle decides. For history rows:

- **O2 is required** for the sampled manifest — the read-back *is* the proof for logical
  equality, and it is not weakened where it is required.
- **O2 is deduplicated by distinct object** before it is sampled. Consecutive states share
  most of their objects, so verifying each distinct object once and assembling each
  file's digest from the verified payload cache is complete and much cheaper than
  decoding the same pack once per file. This is the same mechanism round 5 applies to
  `c2.delta.cdc-locality`, where 110,022 occurrences sit behind ~3,665 distinct objects.
- **O4 is not a read-back.** It compares metadata against the corpus oracle and is cheap,
  so it runs in full.
- **A replay that equals the measured result is not a proof.** The current drivers gate
  "replay root == measured root", which is self-consistency: a replay that faithfully
  reproduced a *wrong* operation would satisfy it. O1 here compares against a **pinned**
  constant, which is what makes it an oracle.

## 6. The storage attribution

O6 is reported as the v0.1.6 campaign reported it, because that shape is what makes a
storage number auditable:

| field | source |
| --- | --- |
| `store_allocated_bytes` | `st_blocks × 512`, `allocation_attribution: exclusive` |
| `store_apparent_bytes` | `st_size` |
| `sqlite_page_count` · `sqlite_freelist_count` | `PRAGMA` |
| `pack_bodies_bytes` | `SUM(length(data))` over `object_packs`, split by `objects.object_role` into file content and metadata |
| `sqlite_nonpack_bytes` | logical bytes minus pack bodies |
| `filesystem_allocation_difference_bytes` | allocated minus logical |

The `object_role` split is what makes the *category gap* — content packs versus metadata
packs versus index and allocation — visible, and it is the number that says whether a
storage change moved the right thing.

Two fail-closed rules carry over from `../c2-families.md` §3.2 and are not negotiable:
`space.py` must assert the table exists rather than use `COALESCE` (a renamed table would
otherwise return `0`, and `0 <= database` **passes** the gate), and a missing table is
`INCOMPLETE`, never a zero.
