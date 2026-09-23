# #237 prospective C5: finalize the one open pooled pack per Save

> **Status: Research; informative and not a product contract.** Frozen before
> product or sparse-probe edits and before C5 samples. The exact-source C3
> dense result remains append-only; the full #229 history gate stays open.

## Source-backed mechanism

C3 exact-closes payload packs and reuses one open 256-KiB pooled-metadata
pack across placement flushes. Its dense 100k Store used 16 pooled packs and
only 142,988 B of pooled tail. A retained sparse-history Store has **157
pooled packs over 157 Saves**: 41,156,608 B capacity, 4,575,113 B declared
used and **36,581,495 B tail**. Its artifact is not a matched C3 sample and
does not prove a C3 result, but it identifies the per-Save risk. Under the
current SQLite `auto_vacuum=NONE` profile, shortening a BLOB frees overflow
pages for *later* inserts without shrinking the file immediately. A labelled
throwaway SQLite diagnostic on the 157 recorded used lengths produced a
41,246,720-B file for fixed rows and a 4,952,064-B file when each row was
shortened before later rows arrived. This is a SQLite mechanism check, not
a LayerFS public operation sample or proof for #229.

## One product change

Keep C3's exact-closed payload lanes and one appendable pooled pack during
each Save. After the final pooled placement flush, inside the Save's existing
final transaction and **before** publication, shorten only that Save's final
open pooled BLOB from its 256-KiB capacity to its header-declared `used`
length with one guarded SQL UPDATE. It must affect exactly the owned pack,
preserve pack ID, group ordinals, body offsets and payload bytes, and fail
through the existing rollback/cleanup path. There can be no subsequent
append after finalization. An uncertain COMMIT outcome follows existing
quarantine; never retry an ambiguous write. No global VACUUM, page-policy
change, format version or work outside the public Save is permitted. The
explicit cost is up to one 256-KiB whole-BLOB rewrite per Save; record it
and measure CPU/RSS/command time. An external test must cover repeated
sparse Saves, same-Save demand, reopened exact readback, and closed SQLite
page/freelist and BLOB geometry.

## Prospective evidence, one sample per arm

Use a shared, committed benchmark-only release example that creates **17
separate public Saves** under frozen default StoragePolicy. Save `round`
(0–16) accepts one `InodeLeaf` of four `RegularFile` values with unique
value seeds `round * 100 + index` (index 0–3), then acknowledges the Save.
The example records the exact 17 canonical IDs/bytes and, in a separate
post-timer reopen, verifies every object. The harness code, compiler,
policy, input schedule and 4-KiB SQLite profile must be byte-identical in
the C3 and C5 arms. Each arm has a fresh Store and output, a locked Cargo
**release** binary and one run. Read the closed Store without writing it:
`st_size`, `st_blocks*512`, page count/freelist, pack version/row/BLOB
length, declared used/tail and all object hashes. Record per-call and
complete command wall, lifecycle CPU/RSS and exact source/harness/binary
identities. This is a focused sparse-pack diagnostic, **not** the 17-state
`history-stride10` operation or a #229 admission result. No previous
history receipt is relabelled.

The shared example is
`core/target/release/examples/issue237_sparse_pooled_probe`. From each
worktree root, use `/usr/bin/time -l` on that release binary with
`--output benchmark-results/issue237-sparse-ARM-17save-20260924-01`;
retain its stdout and time/stderr alongside the output before read-only
geometry. The C3 arm is isolated at source
`5a25f4cf2ef89f0ee2153c53be7e99725ed91423`, which includes the
same example without the C5 product edit. The control output and sibling
stdout/time paths were absent when this selection was made. The exact
shell form, with `ARM` replaced by `c3` or `c5` once in its respective
worktree, is:

```sh
/usr/bin/time -l core/target/release/examples/issue237_sparse_pooled_probe \
  --output benchmark-results/issue237-sparse-ARM-17save-20260924-01 \
  > benchmark-results/issue237-sparse-ARM-17save-20260924-01.stdout \
  2> benchmark-results/issue237-sparse-ARM-17save-20260924-01.time
```

Pin binary hashes, product tree hashes, exact outputs and artifact hashes
in the raw receipts. `/usr/bin/time`'s process peak is lifecycle memory,
not a phase-local peak.

For this diagnostic, C5 must pass full reopened object readback and reduce
combined allocated Store bytes by **at least 3,000,000 B** against the
same-policy C3 arm. Report any miss, the freelist and command/CPU/RSS
tradeoff. The benchmark family's 15-s complete-command and under-10-s
verifier expectations remain visible; no timeout or workload adjustment
rescues a miss. C3's existing dense 100k release receipt is the dense
control. Take one new C5 release 100k call with the unchanged SDK harness,
exact SHAKE manifest and zero-resident payload preflight; require full
101,001-path readback, no worse combined allocation by over 1,000,000 B
and disclose any time/RSS increase. Do not repeat C3 or v0.1.6.

The registered #229 sparse-history guard cannot be closed by these
diagnostics: `history-stride10` already takes 38–40 s versus the 15/25-s
command bounds, prior matched stride1 attempts failed before the first
state root, and its verifier cannot exclude unexpected paths. Report it as
open rather than promoting this smaller Store-level case.
