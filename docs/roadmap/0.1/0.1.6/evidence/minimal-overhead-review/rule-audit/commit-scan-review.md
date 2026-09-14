# #130 adversarial audit: Commit, scans, publication and evidence

Read-only audit, 2026-09-14. Scope: pinned issue130.md and the current synthesis against frozen rules §§3,7,8,10 and corresponding specification. No product or existing-document edit, issue mutation, build, test, probe or benchmark was performed. This new report is the sole output. HEAD observed as `8d6352128248d2598af20641127371f983d66a4e`; another task was editing HostOverlay. The input hashes below are read identities, not an atomic candidate or binary seal.

#130 explicitly starts implementation only after verified terminal closure of #124/#125. This audit does not change that scheduling rule or make an optimization an additional gate for those issues. Assume the delivered prerequisite implementation has qualified V1; the inherited V1 problem is not itself a finding against #130. All findings below concern concrete ways the proposed optimization could regress an already correct predecessor.

## Findings

### CS1 — P2: the public clean shortcut does not distinguish equal filesystem state from changed lineage

**Location:** `rule-audit/issue130.md:83–87`; synthesis `README.md:124–128`. Contrast the more specific prior `commit-analysis.md:53–55`, which restricts the shortcut to the captured logical sequence equaling applied canonical coverage and keeps dirty-net-zero attempts on the builder path.

**Rule:** §7 lines239–244 requires incremental C2 against the last canonical predecessor even when live provenance differs; §8 lines269–279 requires exact expected comparison context and captured coverage. Spec lines420–431 requires exact predecessor-relative spans and forbids full-payload fallback solely because live bases differ. Spec lines448–464 makes Origin occurrence identity distinct from byte/hash equality.

**Concrete trigger:** C1 records a large file with originO and canonical contentR. An ordinary whole-file overwrite installs equal bytes with fresh originP; restore file metadata if needed so the effective canonical state remainsR. The captured sequence nevertheless advances. A shortcut based on equal root/hash/bytes skips the builder, returns UpToDate and advances coverage while cloning the old O→R description. A subsequent one-byte C2 edit now sees live P ranges against published O descriptors. It loses the unchanged anchors and can reconstruct/hash/CDC the full large file even though the final bytes remain correct.

**Consequence:** root equality is not enough to reuse the old correspondence. This violates locality/exact comparison-context obligations, or can cause unsafe substitution if code equates the origins to make them match. It is a design underspecification in the self-contained issue, not a claim that a faulty shortcut is implemented now. Later general text saying “preserve origins” does not define the shortcut eligibility test.

**Smallest correction:** state that the zero-work shortcut requires a complete supported capture in the same Workspace and the same applied comparison context, with captured logical sequence exactly equal to that context's covered sequence (or an equally strong explicit proof that both effective state and correspondence are reusable). A dirty/net-zero equal-root attempt must still construct its captured metadata-only correspondence before stage/coverage acknowledgement; avoiding payload encoding does not authorize reusing stale descriptors. Preserve existing Store Created/UpToDate/head/base behavior; a changed base can create a commit even when content root is equal. Never infer publication from stage absence.

**Required affected check:** extend the existing equal-content fresh-Origin/C1-C2 oracle with equal-byte rewrite → UpToDate → localized C2. Assert the new occurrence is represented correctly, only relevant replacement bytes reach CDC, expected head/base and covered sequence remain exact, and lost UpToDate reply resolves without new capture. Use the delivered #124 oracle and its existing limits; no new numerical performance gate.

### CS2 — P2: canonical-name merge continuation is not a live numeric-cookie continuation

**Location:** `rule-audit/issue130.md:170–201` promises the canonical-name two-stream merge and then describes readdir/readdirplus batches; lines245–246 separately require existing numeric-cookie/live-mutation behavior. The synthesis repeats this at `README.md` “Directory enumeration” and “Snapshot consistency, live cursors and content scans” sections.

**Rule:** §3 lines71–80 requires a snapshot scan to retain its exact captured view; §7 lines230–247 preserves namespace/inode semantics and bounded traversal. Spec lines117–129 requires live identity, supported directory behavior and bounded cursors. The existing live-cookie consumer is `crates/layerfs-workspace/src/host_directories.rs:151–200`; its test lines319–322 explicitly observes a newly created entry after prior EOF using the previous numeric cookie.

**Concrete trigger:** live readdir reaches the end of canonical names, retaining its last numeric cookie/name frontier. A new name lexicographically before the old name frontier is created and assigned a later monotonic cookie. Resuming only the name-sorted lower/upper merge skips that new entry. Pinning the old root to simplify the cursor also skips it and silently changes live semantics; restarting by name can repeat already emitted entries or lose exact partial-reply/FORGET ownership.

**Consequence:** the O(L+K) fixed-view name-merge argument is valid for an internal/captured view, but is not a complete algorithm or cost statement for arbitrary live cookie resumes. The later preservation requirement is correct; the missing piece is an explicit interface boundary between those two orders.

**Smallest correction:** explicitly scope the two sorted-name streams and their O(L+K) statement to a fixed captured/internal directory view. Retain the numeric-cookie-order resume layer for live FUSE, adapting imported lower/name-merge output without replacing issued cookie identities. Count any remaining cookie-index seeks, attribute resolution and host RPC work separately. Keep lower/upper roots paired for a captured scan; live continuation uses the delivered mutation contract rather than becoming a whole-directory snapshot.

**Required affected check:** reuse the existing after-EOF insertion, removal/recreate, partial-page cookie resume and partial-reply cleanup tests; add only the missing earlier-sorting insertion/rename case if the delivered oracle does not already cover it. Also keep a captured scan held across those changes to prove it remains old-view exact. This does not require a new benchmark family or alter POSIX/SDK behavior.

### CS3 — P2: affected natural-overlap performance boundaries are not explicit in the acceptance checklist

**Location:** `rule-audit/issue130.md:396–415` requires a held-builder correctness proof and correctly excludes the hold from timing; lines454–464 select ordinary benchmark families and reuse policy; lines480–488 give acceptance checks. Synthesis `README.md:453–472,499 onward` has the same distinction but does not explicitly carry forward foreground p50/p95/p99 or immediate-post-acquisition write measurements.

**Rule:** §10 lines353–357 explicitly requires acquisition work/latency, foreground p50/p95/p99 during Commit, edit metadata amplification and immediate post-snapshot writes with matched small-workload evidence. Lines366–370 explain that old writer-finish benchmark cases do not establish non-pausing overlap. Spec lines757–773 requires test holds only for correctness, natural overlap for timing, complete API/phase/RPC/lock and resource boundaries, and no added oracle/reopen/fault work in performance receipts.

**Concrete trigger:** a proposed shared pager or batched ownership update causes contention only while canonical construction actively reads/evicts pages. Held-builder tests pause that work, so foreground operations finish. Sequential clean/read/edit benchmarks also pass, while real overlapping writes suffer long waits or excessive immediate COW/eviction work. The selected evidence set can miss the optimization's specific regression despite preserving output bytes.

**Consequence:** the issue's blanket reference to applicable contracts is sound, but its otherwise self-contained acceptance list needs the exact affected overlap receipt. This is an evidence omission, not a demand for a new hard latency threshold or for rerunning every previously passing benchmark.

**Smallest correction:** carry forward the delivered #124 natural-overlap qualification explicitly. When #130 changes shared pager/index/cursor/admission dependencies, invalidate and rerun the affected overlap check under its existing workload/version/settings. Record acquisition, first following write, foreground p50/p95/p99 with sample count, lock/RPC waits, metadata bytes/pages copied and natural construction overlap; preserve Begin/Commit/End and outer-command boundaries. Retain unaffected passes under the existing ledger rules. Do not add a replacement campaign, new percentage threshold, artificial timing hold, or any #122 scenario.

## Covered rules / no additional finding

| Area | Audit result |
|---|---|
| Scheduling | issue130.md:1–16 correctly requires completed prerequisite campaign and closure, and prohibits parallel optimization implementation or deferring required #124/#125 failures. |
| Snapshot/source installation (§3 and supporting root contract) | Issue lines280–346 keep one captured view, exact leased-source comparison, atomic namespace effects and I/O outside brief install synchronization. No recommendation permits stale-root replacement or new live recapture for retry. Root/pager proof remains required; no standalone inherited V1 finding. |
| Changed-only scanning (§7) | Issue lines213–238 preserves current latest-entry indexes, captured interval scans, bounded cursors and necessary ancestor/link work. It separates deletion masks from covered tracking and explicitly requires a spec revision before removing dual indexes. No blanket O(D) or whole-history filter is accepted. |
| C1/C2 and zero anchors (§7) | Compact106byte singleton is only a storage variant; tagged/paged descriptions must use the same Origin and two-pass zero-anchor semantics. No new chunk tree or byte hash is accepted as lineage. CS1 is the missing clean eligibility detail. |
| Canonical formats/pipeline (§7) | Issue lines88–98,114–117,423–424 preserve shared construction, CAS/authentication, CDC/extents, FULL/DELTA, packing/compression and same reader/planner across resident/spilled forms. There is no proposal to skip authentication or choose another canonical schema for tiny data. No additional schema finding identified. |
| Publication and uncertainty (§8) | Issue lines348–390 keeps exact captured coverage, stages, Created/UpToDate witnesses, independent live activity and retry ownership. It rejects missing-stage/equal-content inference and preserves canonical substitution as optional, leased and byte-equivalent. |
| Benchmark custody (§10) | Issue lines433–464 correctly distinguishes single historical samples, nested timing boundaries, fresh sessions vs repeated commits, Init vs Begin, source/public-dispatch identity and macOS/Linux resource domains. It retains exclusions/waivers and no new numerical gate. CS3 requests the specific affected overlap evidence already mandated by the frozen rule. |
| Required regressions | Private page/pager/ownership changes affect corresponding spill/reload, capacity, transition, source-failure and cleanup evidence; shared canonical/admission changes affect applicable Init checks. This is dependency-based invalidation of existing required oracles, not an added #130 gate for earlier issues or blanket routine reruns. |

## Input read identities

- `docs/roadmap/0.1/0.1.6/evidence/minimal-overhead-review/rule-audit/issue130.md`: SHA-256 `671d306711a8f05faf4e9e6b939477cd1ab69e5b2766418c1f2a26600dc3f889`.
- `docs/roadmap/0.1/0.1.6/evidence/minimal-overhead-review/README.md`: SHA-256 `8d0a4dbddbac6ef3eea09f4eb94fbe804e9eb7fc43680790c1c424c82ef1618c`.
- `docs/roadmap/0.1/0.1.6/evidence/minimal-overhead-review/commit-analysis.md`: SHA-256 `7a1f1db2a1a03823525ba11f6efd8f1702e613a56972b3a5bd8594e5a979da8b`.
- `docs/roadmap/0.1/0.1.6/overlay-snapshot-rule.md`: SHA-256 `03d93fc5929ba2b6e99c013e4898a454ddf908c81089c41bb87f9d24d54f543e`.
- `docs/roadmap/0.1/0.1.6/overlay-snapshot-spec.md`: SHA-256 `1e5913da8454cf0aae597f0d5cf6908ab8ebe8e86e212f966e424df799b356da`.

The two proposed predicate/adapter corrections and the evidence correction are recommendations only. No existing report or issue body was modified by this audit.
