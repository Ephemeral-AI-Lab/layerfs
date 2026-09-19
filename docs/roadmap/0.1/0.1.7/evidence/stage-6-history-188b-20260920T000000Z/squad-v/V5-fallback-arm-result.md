# V5 — the fallback arm, run. **−6,402,048 B**, and the whole-file lane now beats v0.1.6

**Measured.** Source pin @66bce8378@ + the working tree. Harness-only: **0 product lines**. Two arms, one
variable apart, same binary, same corpus, @LAYERFS_CONSTRUCTION_WORKERS=1@. **No timing is reported** —
the machine was shared; all figures are bytes and load-independent.

## 1. The arm

V1 proposed it and V4 independently reached the same lever; both are v0.1.6's **own rule**
(@crates/layerfs-layerstack-store/src/objects/admission.rs:441-486@): *use the declared predecessor when
there is one; consult the similarity cache **only when @anchor.is_none()@***.

The T1 squad's four-slot arm applied cross-path candidates to **every** object and displaced 17,141 good
same-path bases. This arm keeps the same-path previous version wherever one exists and spends the
cross-path candidates only where the default arm declares nothing.

@```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
B=$H/target/release/fs-bench-storage-content
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 \
    $B --case history-stride10 --out /tmp/v5def --corpus $C
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 LAYERFS_HISTORY_FALLBACK_PREDECESSORS=1 \
    $B --case history-stride10 --out /tmp/v5fb  --corpus $C
@```

## 2. The result

| arm | apparent | whole-file lane | @no_candidate@ | @trials@ | @prefix_selected@ | @ineligible@ |
| --- | --: | --: | --: | --: | --: | --: |
| default (same-path only) | 57,749,504 | 43,873,480 | 10,732 | 34,488 | 34,439 | 572 |
| **fallback (v0.1.6's rule)** | **51,347,456** | **37,347,557** | 7,326 | 37,886 | 37,797 | 2,585 |
| | **−6,402,048** | **−6,525,923** | −3,406 | +3,398 | +3,358 | **+2,013** |

**v0.1.6's whole-file lane is 38,983,278 B. Ours is now 37,347,557 B — we beat it by 1,635,721 B.**

The native, ordinary and pooled lanes moved **0 B**: this lever is whole-file only, as V2 predicted.

**The watch item V1 flagged did fire:** @ineligible_candidates@ rose 572 -> 2,585. The gain is partly
bought with ineligible probes, and **V1's estimate of 49,124,385 B was 2,223,071 B optimistic** — the
measured arm is 51,347,456 B. The second-order chain-depth interaction is real.

## 3. The residual, decomposed to residual 0

@```
  gap after the fallback arm      +2,031,616   = 1.0412x v0.1.6

  whole-file        37,347,557 - 38,983,278 = -1,635,721   -80.5 %   <- WE WIN
  native             5,695,678 -  3,958,057 = +1,737,621    85.5 %
  pooled-metadata    2,019,419 -  2,240,958 =   -221,539   -10.9 %   <- WE WIN
  ordinary           1,022,795 -    678,339 =   +344,456    17.0 %
  lane subtotal                               +224,817
  pack framing                                 +15,680
  pack total                                  +240,497    11.8 %
  non-pack (objects table etc)              +1,791,119    88.2 %
  ------------------------------------------------------------
  residual                                           0
@```

**The gap has inverted.** It began as **81.87 % a missing base declaration**; it is now **88.2 % the
non-pack row grammar** and **85.5 % the chunk lane**. The whole-file lane — 58.0 % of the gap an hour
ago — is now a lane we **win**.

## 4. What remains, and whether it closes

V4's ordered path, re-based on the measured 51,347,456 B rather than its own 57,749,504 B estimate:

| step | lever | cost | cumulative | vs gate 49,315,840 |
| --: | --- | --- | --: | --: |
| 0 | the fallback arm, **as measured** | **0 product lines** | **51,347,456** | +2,031,616 |
| 1 | **L4 — the C1 chunk cursor** (V2: +160–200 LOC, no amendment) | no ruling | 49,609,835 | **+293,995** |
| 2 | **L5 — the objects row grammar** | **schema amendment** | **48,372,843** | **−942,997** |

**L4 alone is 293,995 B short. L4 + L5 clears the gate by 942,997 B (1.91 %).**

**L5's trade is a ruling, not an optimisation** (V4 §6): an integer reference means one extra btree seek
per chain edge, ~4,096 B per edge on a 4 KiB page, against the 32 bytes it saves per row.

**R1b (the persisted content index, ruling B) is no longer on the critical path.** V4 put it last at
645,974 B for a new table and a @SCHEMA_VERSION@ bump; the fallback arm just recovered 6.4 MB of the same
coverage from the caller side, with no format change at all.

## 5. What this does to the open questions

- **The T1 squad's Stage 7 diagnosis is withdrawn.** V1 and V4 both falsified it independently: v0.1.6's
  stride-10 Store holds exactly 17 commits and exactly the same 44,141 whole-file objects, so there is
  no richer pool. The gap was a **declaration rule**, and 6.4 MB of it was recoverable from the harness.
- **The T1 target of 47,048,435 B is not reachable** from the identified levers; the gate is.
- **#188 should be re-scoped, not closed as not-achievable.**

## 6. Caveats

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (64 paths per state, 1,083 of 86,064 units) and reports
  @INCOMPLETE@ rather than PASS, because a sampled read-back is not the whole claim.
- The fallback arm's @ineligible_candidates@ tripled; the mechanism is not fully explained.
- **L5 is an estimate borrowed from V4's report**, computed on a different baseline; the 942,997 B margin
  should be re-derived on the measured Store before it is relied on.
- The harness change is **uncommitted**; production LOC delta **0**.
