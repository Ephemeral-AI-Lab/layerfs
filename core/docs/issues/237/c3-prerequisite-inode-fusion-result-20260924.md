# #237: direct prerequisite-to-inode memory result

> **Status: Research; informative and not a product contract.** One
> locked-release SDK Init memory diagnostic against a retained control.
> Both full readbacks passed. Source metadata cache remains unqualified,
> so raw call times are not a speed or release-admission comparison.

## Treatment and identities

[`prerequisites`](../../../crates/layerfs-service/src/save/import/namespace.rs)
now builds each ordered `InodeUpdate` while emitting its metadata or symlink
root. It no longer allocates separate metadata-root and content-root vectors
and then copies both into an inode vector. The serial range, metadata memo,
source validation, four file constructors, C1 input, C2 Save sequence and C5
publication are unchanged. The prospective
[plan](c3-prerequisite-inode-fusion-plan-20260924.md) froze a **4-MiB
whole-call peak RSS reduction** as the adoption gate before the product edit.

The retained [control receipt](evidence/c3-entry-memory-20260924/control/receipt.json)
and new [candidate receipt](evidence/c3-inode-fusion-20260924/candidate/receipt.json)
share harness seal
`96e74ab69821c13c33649c5873003151c72d0502a7657980860686047c1614a6`.
Their instrumented product seals are respectively
`c2e50eecfed39038a1d6f943acd6741ebe5fa85c57e57846d12af21428c83c3a`
and `79eef35961f0ccc1a837507871cabe0d40b650465546639933cc5828897c44c1`.
The [freeze](evidence/c3-inode-fusion-20260924/freeze.json) and
[compressed temporary instrument](evidence/c3-inode-fusion-20260924/instrument.diff.gz)
pin the remaining identities. Each arm made one real public
`Client::init_project` call on an independent byte copy of the same seed-1
SHAKE 100k/500-MB source, with fresh Store/History and **0/126,206 resident
source payload pages** at launch. The candidate's separate reopened
[oracle](evidence/c3-inode-fusion-20260924/candidate/verification.json)
passed all 101,001 paths, portable metadata values, every file size and
SHA-256, and all 500,000,000 bytes. The control's oracle also passed.
The raw candidate Store, source copy and complete receipt remain at
`benchmark-results/fs-bench-pro/issue237-inode-fusion-20260924-01/`.

## One-shot memory result

| Measure | Control | Direct-inode candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| Scan-end peak RSS | 76,185,600 B | 75,841,536 B | −344,064 B |
| File-loop-end peak RSS | 106,708,992 B | 105,332,736 B | −1,376,256 B |
| Tree-input peak RSS | 120,143,872 B | 105,332,736 B | −14,811,136 B |
| Tree-build-end peak RSS | 142,229,504 B | 127,057,920 B | −15,171,584 B |
| **Whole-call peak RSS** | **145,686,528 B** | **127,631,360 B** | **−18,055,168 B (−17.22 MiB)** |
| Current RSS at return | 136,904,704 B | 118,849,536 B | −18,055,168 B |
| Raw public-call time, diagnostic only | 5.406411000 s | 5.410089250 s | +0.003678250 s |
| Store apparent / allocated bytes | 519,901,184 / 521,580,544 | 519,819,264 / 527,106,048 | −81,920 / +5,525,504 B |
| Full reopened oracle | PASS | PASS | — |

The candidate avoided **6,464,064 B** of two explicit root-vector
reservations. Its inode vector also reserved **8,888,088 B** instead of
11,534,336 B, because it is created once with the exact entry capacity;
the combined explicit reservation reduction was **9,110,312 B**. The
observed 18,055,168-B process peak reduction is not an allocation-by-
allocation attribution: allocator behavior and the small pre-namespace
shift are not separable. The rise from file-loop high-water to namespace
high-water fell by **16,678,912 B**. See the
[derived arithmetic](evidence/c3-inode-fusion-20260924/comparison.json)
and both phase traces.

The **4,194,304-B** decision threshold was met. Keep the direct-inode
allocation change as a memory improvement. The 3.68-ms higher raw public
time, different pack placement and allocated-file-size increase are reported
without a speed or Store-space claim. Directory/inode metadata cache state
was not qualified; the registered #236 SDK Init path is a separate **debug**
selection, and #229 sparse-history compactness is still open.

This public SDK route generates a new random stack and scope seed inside
each call. An additional small fixed-key
[identity attempt](evidence/c3-inode-fusion-20260924/identity-proof-attempt.json)
therefore produced different stack and root IDs and is marked
`NOT_COMPARABLE`, not an equality failure or a successful exact-root proof.
The full readback proves the logical source; byte-identical canonical root
identity across arms remains untested. No raw timing from that attempt is
used here.

Namespace Save still sets the candidate maximum: **127,631,360 B** versus
105,332,736 B after file construction. The conditional file-job frontier
therefore remains `NOT_RUN`. The candidate is **35,225,600 B** above the
retained v0.1.6 92,405,760-B t1 peak; that cross-identity difference is
context only, not a matched admission comparison. A full bounded fresh
builder remains a separate change with the record-custody, ordering,
validation and page-cache requirements in the
[implementation spec](architecture/bounded-import-implementation-spec.md).

## Source verification

At the restored, probe-free candidate source identity, these commands passed:

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --all-targets --locked -- -D warnings
cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'  # 7 passed
```

The full test run includes the Service native-import, 4,097-directory,
Workspace/History and C2 multi-writer selections. No separate simultaneous
native import plus Workspace Commit load diagnostic was run, so this result
does not qualify aggregate multi-writer RSS or competing-Commit latency.

Curated receipts, build and cache sidecars, phase trace, and hashes are in
[the evidence directory](evidence/c3-inode-fusion-20260924/); its
`SHA256SUMS.json` covers every retained small file. Temporary probes and
runner modifications are removed from product source after this run.
