# The stride3 confirmation — and the harness bug that was blocking it

**Measured.** `PAYLOAD_LEVEL = 3`, `GROUP_LEVEL = 19` (the sweep's balance point), `LAYERFS_CONSTRUCTION_WORKERS=1`,
one sample per tier, sequential under `/tmp/lane.lock`. Both Stores re-verified with `space.py`,
`quick_check = ok`.

## The confirmation

| tier | ours | v0.1.6 | vs v0.1.6 | delta |
| --- | --: | --: | --: | --: |
| **stride10** | **49,053,696** | 49,315,840 | **0.99468x** | **+262,144 below** |
| **stride3** | **61,767,680** | 64,000,000 | **0.96512x** | **+2,232,320 below** |

**Both tiers PASS and both beat v0.1.6.** The guardrail's *"iterate on stride10, confirm on stride3"* is met.

@```
  stride10   47.14 s real   39.63 s CPU   262.2 MB RSS
  stride3    97.94 s real   90.00 s CPU   270.7 MB RSS
@```

**Stride3 lane by lane, and it is the same shape as stride10:**

| lane | ours | v0.1.6 | delta |
| --- | --: | --: | --: |
| **whole-file** | 45,818,945 | 48,671,198 | **-2,852,253** |
| **native** | 5,565,067 | 5,687,211 | **-122,144** |
| **pooled-metadata** | 2,720,177 | 3,349,966 | **-629,789** |
| ordinary | 1,770,138 | 1,356,028 | +414,110 |
| **pack blob** | 56,192,535 | 59,339,891 | **-3,147,356** |
| non-pack | 5,575,145 | 4,660,109 | +915,036 |

**Three of four lanes win on both tiers**, and the loss is the same one: the ordinary lane, plus the
non-pack row grammar. **The result generalises across the tier, which is what a confirmation is for.**

## The finding: the stride3 failure was a HARNESS bug, and the guardrail anticipated exactly this shape

**The first stride3 run failed** with:

@```
  g1.o1-chain-complete  INCOMPLETE | product error: ResourceUnavailable { what: "ordering backing" }
```

It was **not** a product defect and **not** a depth or capacity limit. The driver decided whether to
supply the operation's ordering scratch by **binding count**:

@```rust
let mut backing = (bindings > FilesystemResources::default().maximum_pending_records)
    .then(|| FileBacking::new(&backing_directory));
@```

**But the spill is triggered by the pending ROW MAP, not by the binding count**
(@references/reduce.rs:248@: @if self.pending.len() >= self.maximum_pending@). And on an **update** the
walk accumulates rows while descending the **base** tree — so a state can carry far fewer changed bindings
than the 4,096 ceiling and still open a run. Stride3 is 2.99x stride10's declared bytes
(1,676,767,835 against 561,010,345), which is why it crossed the line and stride10 never did.

**Fixed harness-side** — the backing is now always supplied. Supplying one does not *cause* ordering I/O;
it only makes it possible when the operation decides it needs it, **which is why stride10 is unchanged.**

**The same heuristic exists in @ops/fs.rs:475-478@, which drives the 217-row lane.** It is **not fixed** —
that lane is not this round's to change — and **it is flagged as a latent bug of the same shape**: any
registered case whose update walk accumulates past @maximum_pending_records@ with few changed bindings will
fail the same way.

## Not claimed

- **The verify phase is still a declared sample reporting `INCOMPLETE`.** "PASS gates=2" is
  @g1.o1-chain-complete@ plus @g4.swaps@ — **not a read-back of the stored trees.**
- **The machine was not fully quiet** (load ~5, one `cargo` process); the wall and CPU figures are
  therefore indicative, not clean. Bytes are load-independent and stand.
- One sample per tier, no best-of.
- `history-stride1` was not run, per the guardrail.
