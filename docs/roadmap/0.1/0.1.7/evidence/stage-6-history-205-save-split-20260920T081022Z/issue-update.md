### #205 — the save path attributed in time, in seven disjoint buckets (L53)

`storage.accept_loop` is one span around `for id { operation.accept(object) }`, is 50–55 % of a `history.*` row's operation, and had **no time attribution at all**. It now has one: `SaveProfile` charges seven disjoint nanosecond buckets at the call sites that do that work, accumulated into `OutcomeCounters` and published per state as `history.state.<n>.save.*_ns`. It is an aggregate and never a span per object, because a stride1 row accepts ~10^5 objects and the driver refuses a per-object timer node. `resolve` is itself split five ways, and the parts are asserted to sum to it exactly on every row.

**The split, in seconds.** stride10 (17 states, operation 16.296 s, scope 9.312 s): resolution 4.247 (45.61 %), FULL 1.599, delta 0.637, group codec 0.044, placement 0.262, SQL 1.068, commit 0.421, **remainder 1.034 (11.10 %)**. stride1 (157 states, operation 101.994 s, scope 47.821 s): resolution **36.458 (76.24 %)**, FULL 3.348, delta 1.403, group codec 0.151, placement 0.467, SQL 2.104, commit 0.882, **remainder 3.008 (6.29 %)**.

**Resolution, five ways, stride1:** reuse verification **16.544 s (34.60 % of scope)**, pooled lane **8.086 s**, base acquisition **6.633 s**, eligibility walk **5.186 s**, post-trial cost walk **0.008 s**. Per object early → late: reuse 1.99×, pooled **2.53×** (the only part outgrowing the scope's 1.73×), eligibility **3.48×** (fastest). At the coarser row the order inverts — there the pooled lane is 59.5 % of resolution and reuse is 2.25 % — which is exactly why the split needed both ends of the curve.

**Two long-standing unknowns closed.** The group codec costs **0.151 s at stride1** (0.32 %): L40's +1.685 s was the *delta* of level 19 against level 1, so codec level is not a lever worth one second. The post-trial cost walk is **8 ms**: L51's depth-cap binding is a policy fact, not a time cost at that site.

**And the correlation is retired.** The +0.947 / +0.805 / +0.409 elapsed-versus-count correlations no longer carry attribution; the buckets above replace them. The cross-arm differences against the retained uninstrumented samples (−0.612 s stride10, −3.732 s stride1) have the **wrong sign to be an overhead bound** and are reported as reproducibility, not as a saving; the one clean within-build bound is +0.048 s on stride10 for the refinement, which charges the same number of clock pairs.

**The treatment decision, per the owner's fixed order.** The split named B1 (do not re-verify what this save already verified), and the brief recorded that the count it needs — how many reuse occurrences repeat an identity verified earlier in the same operation — did not exist. It was counted with a measurement-only probe (`LAYERFS_STORAGE_REUSE_PROBE=1`, off by default, observing the completed verification and changing no decision, byte, row or root) and it is **zero**: 121,301 reuse occurrences at stride1, 121,301 observations, **0 repeats**; at stride10 the per-call trace shows 1,121 of 1,121 reached and 0 repeats. The probe's own cost is 13.1 ms for 121,301 insertions.

So **B1 is not pre-registered, not implemented, and no saving is claimed for it** — a wave-scoped and a whole-operation memo would both save exactly nothing, because a repeated identity within a wave is already answered by the wave-local `prepared` map and one across waves is in the wave's own `by_id` snapshot. L52's 9.21× growth is growth in *distinct* identities, not in repeated work.

**No product behaviour change ships from this round.** Equivalence holds where it must: both instrumented rows reproduce the recorded Store constants exactly (`4af37932a…`, `1635cf7bb…`) with unchanged state roots and canonical inventory. Checks: core fmt/clippy/test exit 0 (**494 passed / 0 failed**), boundary PASS + self-tests OK; harness **117 passed / 0 failed**. Production LOC **85926 → 85978 (+52)**.

**Next, with no treatment claimed:** the pooled lane's resolution (largest at stride10, second at stride1, only part outgrowing the scope) and the eligibility walk (fastest-growing; the winning candidate's chain is walked to measure depth and then walked again to rebuild it). **#205 stays open.**

### #190 — read path untouched

This round changes no read-path line: the depth term L49/L51 treated and L52 reinterpreted is unchanged, and the `resolve` split above names it inside the *save's* accept path. The one figure that touches #190's subject: at stride1 the eligibility walk now costs **5.186 s** and base acquisition **6.633 s**, both inside `storage.accept_loop` rather than in the read path L49 removed. **#190 stays open** — it still needs the owner's D1–D5 qualification rulings and the v0.1.6 reconciliation, neither of which is this lane's to supply.

## Why the save path is slow on long histories — and why the successor should go to FUSE next (L53 continuation)

This section is the answer to "does it get slower with longer histories, and which operations cause it". It is measured on this round's own arms; every number is in the evidence above or in `runs/`.

### The operations the workload performs

The driver's changed set has exactly three non-empty kinds on this corpus: **construct** (`Added`), **edit/write** (`Modified`), **delete** (`Removed`). `MetadataOnly` is **0 in all 157 checkpoints** — counted, never dropped. Across stride1: **46,029 constructs / 135,003 edits / 36,614 deletes**, i.e. **edit is 62 % of paths and 82 % of bytes**, construct 21 % of paths, delete 17 % of paths carrying no stored content.

### Slowness is a volume term on construct and edit, not a per-write cost

| | stride10 | stride3 | stride1 | growth |
| --- | --- | --- | --- | --- |
| objects written | 52,032 | 72,560 | 97,788 | 1.9× |
| reuse events | 1,121 | 21,653 | 121,301 | **108×** |
| changed paths (file ops) | 52,417 | 91,445 | 217,646 | 4.2× |
| reuse events per object written | 0.022 | 0.298 | 1.240 | **56×** |

Per state at stride1: `r(reused, modified paths)` = **0.813**, `r(reused, added paths)` = **0.793**, and `r(reused, inserted objects)` = **−0.05**. Reuse has **no** positive relationship with how many objects the save writes; it tracks **file-level constructs and edits**. At stride1 reuse events (121,301) exceed changed paths (217,646) by 0.56 per path. That is the mechanism: a construct re-offers content the Store already holds, triggering the full chain rebuild + identity re-hash + byte comparison.

### The per-operation decomposition, and where the remaining unknown is

Two distinct construction terms, measured separately by running the harness's own declared control (`LAYERFS_HISTORY_CHUNK_PREDECESSORS=0`):

| stride10, one state set | operation | content | filesystem | accept_loop |
| --- | --- | --- | --- | --- |
| predecessors on | 16.296 s | 1.585 s | 5.146 s | 8.820 s |
| predecessors **off** (control) | **10.882 s** | **1.039 s** | 3.490 s | 5.847 s |

So of `content`, **~0.55 s is prior-state resolution read out of the Store**, not construction; **1.039 s is the actual construction** (encode + chunk + hash + emit) of 259 MB of changed file bytes that are **already in memory** — no disk read — and its **per-changed-path cost is flat at 24–40 µs across all 17 states**. Content construction does **not** degrade with history. The tree term (`filesystem`, 3.49–5.15 s) is the larger and the growing one.

And the base-offering policy is the **single largest lever found so far**: declining to offer prior-state bases cut the stride10 operation by **33 %** (16.296 → 10.882 s) and `accept_loop` by 34 %, at the cost of a larger Store. Both axes must be reported if it is ever treated.

### FUSE is the successor's highest-value next step, and here is the measured reason

v0.1.6's own recorded `create` phase (= `create_workspace_session`, the FUSE mount) is **9–11 ms** for a workspace with a 100-commit branch behind it, and it is **flat across depth**: boundary-cycle 11.14 → 9.14 ms (K10 → K100), large-hotset 10.17 → 9.77 ms, namespace-inode 9.90 → 9.82 ms. That is the "under 20 ms" figure, confirmed from `issue154/final-complete-matrix.json`. It does **not** materialise content: `create_workspace_session` pins the branch, creates a state dir, attaches the projection and reads initial tree metadata. Construction on the legacy side happens in Commit/push.

The core currently pays eagerly, inside the measured operation, for what a FUSE design defers:

| | v0.1.6 `create` (mount) | core `content` + `filesystem` |
| --- | --- | --- |
| work | branch pin + dir + FUSE attach + tree-metadata read | **eagerly constructs every changed path's canonical objects** and rebuilds the tree per state |
| cost | 9–11 ms | 1.04–5.15 s per state |
| scales with history | **no** (flat K10→K100) | yes |
| when content is served | lazily, on first read | inside the operation |

**FUSE relocates this cost; it does not delete it.** A FUSE-backed core still has to construct and accept the objects for whatever it serves. Ingesting this lane's full history (4,936,693,030 bytes of cumulative logical history over 904,143 path-states) still pays construction and admission; FUSE makes *serving* lazy, not *storing* lazy. FUSE wins on workloads that read a small fraction of what they store — which is v0.1.6's shape — and loses on workloads that read everything, where it adds fault and round-trip overhead on top of the same construction.

**So the successor's directive is to land FUSE and then measure first-read latency after mount.** That number is missing on *both* sides: v0.1.6's `historical_access` family — the operation that would have recorded its deferred cost — declares performance **`N/A`** in all six cases, and the core has never run a mount-at-depth or read-after-history case. Until it exists, "10 ms vs 1–5 s" compares a mount against a materialisation of a different workload, and no speed claim in either direction is supportable.

Concrete asks for the FUSE round, in priority order:

1. **A mount node**, so the core has a `create`-equivalent to compare against v0.1.6's 9–11 ms flat figure. The measured neighbours suggest single-digit-to-low-double-digit ms (`store.create` is 4–5 ms), but that is an expectation to test, not a number to quote.
2. **First-read-after-history**, the unmeasured half of the trade, with the read fraction declared. This is the case that decides whether laziness helps or hurts, and it is the only way to answer "would v0.1.7 achieve the same speed".
3. **Decide whether the eager `filesystem` cost is intended.** It is what makes the Store dedupe a whole history in one operation — this lane's actual claim — so it is not obviously a defect, and nobody has established which bill is cheaper.

Bounds unchanged: all `history.*` rows are diagnostics, admission `INELIGIBLE`, every budget class `NOT_RUN`. The base-offering and deferral questions are owner decisions; this section recommends, it does not rule. **#205 and #190 stay open.**
