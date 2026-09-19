# Verifying a retained-history row

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §6
> and §11, and by [`../gates_and_oracles.md`](../gates_and_oracles.md).

Verification is a **second, unmeasured invocation** with its own declared budget. It reopens
the Store the performance invocation left behind and reads the history back out of it. It
never enters a performance distribution, and a verifier failure preserves the performance
sample while failing the row's admission.

## 1. The final gate

One gate, over every state of the selection:

```text
for every state k in the selection:
    the root read back from the Store equals the pinned root for state k        O1
    the tree read back from the Store equals oracles/<sha_k>.json              O4
    a deterministic sample of its files reads back byte-exact                  O2
the Store is well formed: schema, tables, indexes, watermark, quick_check      O7
the storage readings reconcile with the published attribution                  O6
```

**Any disagreement fails the row.** A single state that does not read back is not "156 of
157" — it is a failure, and it is reported as one.

## 2. The oracle set

| oracle | what it checks here | coverage | cost |
| --- | --- | --- | --- |
| **O1 identity** | each state's filesystem root `ObjectId` equals its pin | full, N states | O(1) per state |
| **O2 logical equality** | each sampled file's bytes read back **through the Store** and compared to the corpus oracle's sha256 | **deterministic 10 % by default** | proportional to bytes and chain depth |
| **O3 structural count** | canonical bytes, object count and pack count pinned | full | O(1) |
| **O4 tree equality** | each state's entry manifest — path, mode, size, plus directory entries — equals `oracles/<sha>.json` | full, N states | metadata only |
| **O6 footprint** | allocated vs apparent, `pack_bodies <= database`, freelist, sidecar absence, attribution | full | O(1) SQL and `stat` |
| **O7 SQL invariants** | schema identity, 4 tables, 2 indexes, watermark `I1`, `quick_check == ok` | full | O(1) |

Only O2 is sampled, because O2 is the only oracle whose cost scales with the history: v0.1.6's
verification wall was 199.8 s on 53 states and 570.6 s on 157, against 306,861 and 904,143
path-states. O1, O3, O4, O6 and O7 are O(1) or metadata-cheap and carry the claim.

## 3. The deterministic sample

The harness's existing rule, stated once and applied wherever a row declares a countable unit:

```text
max(1, ceil(n/10)) units, selected by index % 10 == 0 in declaration order
```

**Declared unit: the state's file manifest** — the same list O4 compares — in corpus order.
Not the object count, not the pack count, and never "the first ten".

**Endpoints are included.** This lane requires the first and last manifest entry of every
state in the sample, because that is where boundary defects live. The current implementation
(`src/ops/mod.rs` `sampled_indices`) takes every tenth index with `take(ceil(n/10))`, which
for 53 units yields 0, 10, 20, 30, 40, 50 and **omits the last**. That is owner decision 4 in
the [README](README.md#7-owner-decisions-still-open); until it is ruled, a sample under this
lane is not admission evidence.

The selection rule is named in the receipt, so a sample is reproducible from the receipt
alone. Every receipt carries `verification_mode`, `verification_declared_units`,
`verification_sampled_units`, `verification_selection`, `verification_omitted` and, when a
proof was reused, `reused_proof_identities`.

## 4. Where the pins come from

**(a) Corpus pins — available immediately, fully independent.** Path-states and logical bytes
are properties of the corpus, checked against it directly:

| row | verified path-states | verified logical bytes |
| --- | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 |
| `history-stride3` | 306,861 | 1,676,767,835 |
| `history-stride1` | 904,143 | 4,936,693,030 |

**(b) Cross-generation canonical pins — available immediately, and the strongest check this
lane has.** Recorded from v0.1.6 on byte-identical source trees, and preserved by v0.1.7's
canonical-identity policy:

| row | canonical content | canonical objects |
| --- | --: | --: |
| `history-stride3` | 589,423,458 B | 73,476 |
| `history-stride1` | 871,588,115 B | 104,705 |

**(c) First-run pins — legitimate, and only because of what happens next.** The per-state root
identities do not exist yet, and recomputing them off the product path is not independent. So
the first full run establishes them and thereafter the row must **reproduce** them, with a
counter that moves being a `FAIL` rather than a new baseline. This is the mechanism
`src/workload/expected.rs` already uses for the 217, whose constants were taken from the last
all-`PASS` run and compiled in with `include_str!` so the harness identity covers them. The
same applies here: the history table is embedded, not read at runtime.

## 5. The mode ladder

| mode | oracle | status it can produce |
| --- | --- | --- |
| `full` | every oracle, O2 over every manifest | `PASS` |
| `sample` | O1/O3/O4/O6/O7 full, O2 over the deterministic 10 % | **owner decision 3**: `PASS` for this lane only, `INCOMPLETE` elsewhere |
| `none` | nothing | `INCOMPLETE`, never `PASS` |
| `reused` (`--reuse-pass`) | one identity-matched `status=PASS` receipt | `PASS` |

`sample` is the proposed default for these rows, and the reason it can be is that the storage
claim is not sampled: the O(1) counters that decide storage are read in full in every mode.
The 217-row contract keeps `full` as its default, because flipping it there is a change after
collection and would let a 10 % oracle admit rows whose frozen oracle is O1+O2.

`--reuse-pass` accepts one identity-matched receipt instead of re-running verification. It
fails closed on schema, identity, hard-limit and wall mismatch, and records
`reused_proof_identities` plus an explicit omission. `--skip-verification` exists for
iteration and marks every row it touches `INCOMPLETE`, never `PASS`.

## 6. What is not required, and what is

- **O2 is required** for the sampled manifest — the read-back *is* the proof for logical
  equality, and it is not weakened where it is required.
- **O2 is deduplicated by distinct object** before it is sampled. Consecutive states share
  most of their objects, so verifying each distinct object once and assembling each file's
  digest from the verified payload cache is complete and far cheaper than decoding the same
  pack once per file.
- **O4 is not a read-back.** It compares metadata against the corpus oracle and is cheap, so
  it runs over every state.
- **A replay that equals the measured result is not a proof.** The existing drivers gate
  "replay root == measured root", which is self-consistency: a replay that faithfully
  reproduced a *wrong* operation would satisfy it. O1 here compares against a **pinned**
  constant, which is what makes it an oracle.

## 7. Read amplification

Verification reports decoded bytes against requested bytes, as v0.1.6 did — its cold range
amplification was 22,216,028 decoded bytes for 6,421 requested. A read path that decodes a
whole pack to serve one file is an algorithmic finding, and this is the counter that shows it.
It is reported, and it is a gate where the specification pins a bound.

## 8. The storage attribution

O6 is reported as the v0.1.6 campaign reported it, because that shape is what makes a storage
number auditable:

| field | source |
| --- | --- |
| `store_allocated_bytes` | `st_blocks × 512`, `allocation_attribution: exclusive` |
| `store_apparent_bytes` | `st_size` |
| `sqlite_page_count` · `sqlite_freelist_count` | `PRAGMA` |
| `pack_bodies_bytes` | `SUM(length(data))` over `object_packs` |
| canonical bytes and object count by `object_role` | `SUM(canonical_length)` and `COUNT(*)` over `objects`, grouped by role |
| `sqlite_nonpack_bytes` | database logical bytes minus pack bodies |
| `filesystem_allocation_difference_bytes` | allocated minus apparent |

The `object_role` split is what makes the content-versus-metadata category view visible, and
it is the number that says whether a storage change moved the right thing. It is taken in
**canonical** terms — `objects.canonical_length` grouped by `objects.object_role` — with pack
framing reported as one separate overhead number. Attributing *pack* bytes to a role would
require decoding the pack directory, because a pack blob holds many objects and a naive join
would multiply-count them.

Two fail-closed rules carry over from [`../c2-families.md`](../c2-families.md) §3.2 and are
not negotiable: `space.py` must assert the table exists rather than use `COALESCE` (a renamed
table would otherwise return `0`, and `0 <= database` **passes** the gate), and a missing table
is `INCOMPLETE`, never a zero.
