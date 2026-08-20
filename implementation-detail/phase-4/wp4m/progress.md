# WP4-M optimization progress

## Current terminal status — CP-0006

WP4-M fixed-radix compact lane status (2026-08-21): **PASS / COMPLETE;
WP4-P eligible but not complete; compatibility promotion=false**. CP-0006
completed 27/27 rows in an observed 50-second console wall under the configured
120-second ceiling: six warmups, 18 measured rows, and three complete
roundtrips. Python and Ruby independently return PASS with no reasons. The
100-MiB medians are full write `603.327666 ms`, same-count middle
`8.639167 ms`, `+1` early `432.939417 ms`, and `+1` middle `432.324667 ms`.
Maximum observed Q is `2,222,803` bytes and all 27 rows end at zero. The raw,
Python, Ruby, executable, and runner hashes are respectively `b3596ff6...72e1`,
`d080f0f8...4f5`, `86cd7018...7114`, `7e91b90f...dbb36`, and
`965cc07f...40c25`.

K64/F64 is policy-selected and DIR256K is the unmeasured fallback, but neither
is compatibility-promoted. The next and only compatibility-bearing step is
WP4-P: delete losers/selectors, regenerate and fingerprint selected-only
goldens, and pass the promotion audit. WP5+ remains blocked until WP4-P
completes. No further WP4-M campaign or 512-MiB run is required.

## Historical 216-row campaign — original-contract NO-GO / custody lost

WP4-M profile campaign bounded-close status (2026-08-20): **216-row campaign
and 252-database audit COMPLETE; correctness/storage evidence PASS,
performance FAIL; terminal custody INCOMPLETE; overall WP4-M acceptance not
claimed; WP4-P ineligible**. The final private matrix completed 216/216
planned/started/returned/raw processes
(36 warmup, 180 measured), zero child failures, exact Q terminal zero, two
agreeing independent analyzers, and a 252/252 read-only SQLite/storage audit.
No file challenger reaches the 5%/4-of-5 primary gate; both reverse at 512
MiB. No directory challenger reaches the replacement primary gate. All file
profiles fail forced-`+1` at 61.997–71.417% of full capture versus the 5%
limit. K64/F64 100-MiB durable capture is 706.598 ms / 141.523 MiB/s versus
accepted F2-v3 659.593 ms / 151.609 MiB/s. K64/F64 and DIR256K remain defaults
only by the then-frozen fallback; that campaign selected/promoted nothing. The leading
directory row starts at 99,999 and finishes at the governing 100,000-child
limit. Final executable is `925dff2d…230ec`; campaign source is
`4e3b8e1f…ca1cc`. Under that historical contract WP4-P was ineligible; CP-0006
now supersedes only that eligibility result. The preregistered
complete manifest/seal over the 65-GiB root was NOT RUN under the user's
runtime cap; no partial manifest substitutes for it, the root is not sealed,
and external attestation/final audit are unavailable. Existing no-go findings
are directional evidence, not a sealed promotion checkpoint.

F4-A2 terminal status (2026-08-20): **VALID / NO-GO; retain accepted F2-v3;
close format-preserving F4**. The exact same-gear diagnostic compared accepted
scanner-owned complete-chunk materialization (A) with boundary-only plus all
required bounded carry work (C) under adjacent `AC/CA/AC/CA/AC` pairs; B ran
later as supplemental boundary-only evidence. Net A-C budgets after the
prospectively frozen `0.397875 ms` observer ceiling are
`3.701583/1.363542/3.076167/5.517375/4.210667 ms`: median `3.701583 ms`,
0/5 at the 33-ms gate. Exact 104,857,600 bytes, 3,201 reads, 5,284 boundaries,
and CDC fingerprint `5bb376c3…5994` pass in all 18 rows. C copies exactly
67,072,778 bytes for 3,200 carry-required chunks using 7,343 calls, one
32,768-byte buffer, and terminal heap zero; median direct carry wall is
1.906844 ms. Eight focused and 121 workspace tests, static checks, release
self-test, and two-analyzer agreement pass. Accepted source is restored at
`c8ac86be…cc158`; no optimization is retained. F4-B/F5/F6 remain ineligible;
any broader physical-profile experiment requires explicit authorization.

F4-A terminal status (2026-08-20): **VALID / NO-GO; retain accepted F2-v3**.
From clean documentation checkpoint
`83d085bd80e82ae22b4a9766f2fc8aed03501fb8`, one warmup and five measured
accepted-path rows partition mapping and standalone COMMIT without double
counting. Medians are mapping 524.111750 ms; source 16.468330; CDC 128.723024;
three distinct required hash lanes 95.185147/89.067215/96.068155; encode
3.161540; bind 1.385969; transient-copy upper bound 2.745299; explicit copy
zero; mapping VDBE+pager 48.853618 and VFS 24.281657; COMMIT 112.144334 with
VDBE+pager 18.199272 and VFS 93.030990 ms. The large hash/CDC/VFS lanes are
required work, not removable budget; stripped system SQLite leaves VDBE/pager
an ineligible composite. The only eligible explicit-copy lane is zero in 5/5,
so no mechanism passes the 33-ms/4-of-5 gate. All identities, one COMMIT,
pager/storage, six integrity checks, Q terminal zero, tests, and source
restoration pass. Do not optimize, start F5/F6, change profile/schema/
durability, resurrect F3, build a carrier, integrate production, or commit.

F3 causal-diagnostic closure (2026-08-20): **VALID / NO-GO; retain accepted
F2-v3; F4 ineligible**. The earlier universal-exhaustion wording is narrowed:
v1/v2/v3 tested three classification shapes at the same R64/B1MiB envelope,
not the whole cap/transport curve. Sealed D1-v1 is `REVISE` after one warmup
control row exposed an extra report brace. D1-v2 changed only strict JSON
publication, then completed 34/34 full-create and 4/4 M4.5 rows. Light mapping
is `530.074166 -> 528.057708 ms` (-0.380% arm, -1.005% paired, 3/5), while
durable capture is `712.157750 -> 717.935708 ms` (+0.811% arm, +1.407% paired,
2/5) and throughput `140.418 -> 139.288 MiB/s`. RSS/footprint fail at +5.149%/
+5.164% arm and about +5.371% paired. The direct VFS proves one candidate
statement-subjournal spill per measured row: 3,946 writes, 8,216,400 requested
bytes, 16.127082 ms median nested callback wall; control records zero. Binds and
BLOB bytes remain exact/unchanged; post-bind statement MEMUSED rises 52,176 ->
1,256,000 bytes; logical Q rises 55,325 -> 1,147,173 and returns to zero. Pager,
storage, identities, one COMMIT, schema/residue, and M4.5 all pass. Primary and
independent analyzers return valid `NO-GO`: no measured mechanism supplies the
frozen 60-70 ms credible budget for F3-v4. Preserve both D1 roots; do not build
v4, start F4, tune caps, integrate production, or commit.

F3 terminal status (2026-08-19): **FAIL / revert; F4 ineligible**. Three
prospectively versioned bounded immutable-CAS insertion shapes all prove the
requested `5,372 -> 103` SQLite INSERT-execution reduction but fail the frozen
performance/resource contract. V1 `RETURNING` regresses mapping/durable by
12.068%/10.450%; v2 removes all returned inserted-ID rows but still regresses
3.885%/4.141% and its final audit finds a wrong-kind later-duplicate gap; v3
repairs that P0 and reaches the lower-bound fresh path of 103 optimistic
INSERTs with zero group queries, fallbacks, or result rows. V3 nevertheless
regresses mapping `489.054 -> 521.492 ms` (+6.633% arm, +6.661% paired, 0/5)
and durable capture `653.849 -> 693.111 ms` (+6.005% arm, +5.767% paired,
0/5); RSS/footprint also fail at +5.430%/+5.429%. Exact roots, transition,
closure, 5,372 objects, 105,291,554 canonical bytes, 365,262 mapping bytes,
5,373 changed rows, 10,748 row-BLOB writes, 26,676 dirty writes, 6,675 spills,
FULL+DELETE, one transaction/COMMIT, storage endpoints, post-COMMIT work,
release M4.5, and Q `1,147,173 <= 1,310,720` with terminal zero all pass.
VFS/xSync/physical-byte causality remains Unavailable. Preserve v1, v2, the
initial v3 permission-mode failure, and complete v3-r1 artifacts; restore the
accepted F2-v3 source; retain additive reports only; make no commit and do not
start F4, profile selection, production integration, backend work, or cap
tuning.

F2-v3 terminal status (2026-08-19): **PASS / retain; F3 eligible only as a
separate reviewed task**. The prospectively frozen same-binary diagnostic
isolated verifier-dependent pager/work redistribution and authorized the v3
combined-tail contract without relabeling v1/v2. The single frozen acceptance
campaign used schedule `AB/AB/BA/AB/BA/AB` and byte-identical pair bases.
Pre-COMMIT queries fall `5,373 -> 1` and BLOB/authentication `5,373 -> 0`;
durable capture improves `916.310 -> 659.593 ms` (`-28.016%`, paired
`-27.725%`, 5/5) and qualification+COMMIT improves
`512.861 -> 168.477 ms` (`-67.150%`, paired `-67.513%`, 5/5). Exact
identities, CDC/root/transition/fresh closure, writes, FULL+DELETE, one
transaction/COMMIT, final pager equations, schema/storage, reconstruction,
ranges, and M4.5 pass. Total CPU improves 16.049%; RSS/footprint/store pass;
system CPU is +30 ms within the frozen +60-ms ceiling; tiny phases pass their
pre-row 200-us envelope. Candidate Q is exactly `55,325 <= 73,728` with
terminal zero. Independent Python/Ruby statistics agree exactly and both
return PASS. Standalone COMMIT remains reported at `126.054 -> 168.426 ms`
(`+33.614%`) as a phase-coupled diagnostic; VFS/sync/physical causality is
Unavailable. No F3, profile selection, production integration, schema/backend
change, or commit was performed.

F2-v2 terminal status (2026-08-19): **FAIL / REVISE; F3 ineligible**. The
standalone authority/Q/hash/cleanup repair passes 13 focused and 113 workspace
tests, static gates, exact identities/work/storage/one-COMMIT, absolute Q
`55,325 <= 73,728` with terminal zero, and the release M4.5 regression
(`433.194708 -> 8.422917 ms`). Against sealed F1-v3, pre-COMMIT queries fall
`5,373 -> 1` and BLOB/auth `5,373 -> 0`; durable capture improves
`916.758 -> 652.573 ms` (`-28.817%`, paired `-28.505%`, 5/5). Acceptance
still fails: COMMIT regresses `129.875 -> 164.052 ms` (`+26.315%`, paired
`+25.653%`, 0/5), fresh-reopen arm median is `+6.593%`, ranges are only 3/5,
and the additional v2 relative-Q gate is 0/5. A diagnostic-only 200-ms idle
does not repair nested SQLite dispatch (`167.886 -> 160.304 ms`, paired
`-1.082%`, one +6.629% pair); physical causality remains Unavailable. Preserve
`target/wp4m-f2-construction-proof-k64-20260819-v2`; do not start F3, batch,
select/promote, integrate production, add metadata/backend, claim Phase 4
complete, or commit.

F2-v2 continuation status (2026-08-19): **IN PROGRESS; F3 ineligible** from
clean checkpoint `4d20b7c` / tree `9355b1af`. The immutable F2-v1 evidence is
corrected additively by `f-series/f2/v1-audit-addendum.md`: besides
the recorded COMMIT failure, fresh reopen passed only 3/5 protected pairs and
ranges passed only 3/5; v1 environment/toolchain/build/test-output custody is
Unavailable. V1 also depended on an external root/transition/closure oracle
and did not establish exact live-overlap Q.

The current uncommitted v2 repair removes external root/transition/closure
from proof issue/consume, constructs singleton workspace and Genesis edges
inside the proof fold, leaves flat closure computation to fresh post-COMMIT
verification, removes the redundant per-chunk rehash, assigns the exact
frontier charge to its `FileBuilder` owner through unary/root finalization,
uses a nonallocating level scan, and routes every F2 post-BEGIN failure through
`transaction_attempt`. The expanded 48-test workspace result currently passes
(44 core + 4 engine + 48 private benchmark + 12 parity + 5 eval = 113),
including no-oracle, corrupt-golden, full binding/replay/lifecycle/overflow,
incumbent role/length/malformed/missing/unequal, namespace/transition, unary
collapse, and terminal-Q cases. This is debug correctness only: no v2 release
binary or timing row exists yet, F2 is not accepted, and F3 remains ineligible.

F2 status (2026-08-19): **FAIL / REVISE; F3 ineligible**. The private bounded
full-create construction proof passes shadow equivalence, authority,
adversarial, exact-Q, storage/schema, one-COMMIT, and release M4.5 gates. It
reduces pre-COMMIT SQL queries `5,373 -> 1` (`-99.981388%`) and BLOB reads/
object authentications `5,373 -> 0`; durable capture improves
`929.420 -> 786.868 ms` (`-15.337802%`, paired median `-15.628724%`, 5/5),
CPU/RSS/peak/storage pass, and Q is `55,325 <= 73,728` with terminal zero.
The prospectively protected COMMIT phase regresses
`135.886 -> 176.823 ms` (`+30.125789%`, paired `+28.184244%`, 0/5), so no
threshold is relaxed and F2 is not accepted as the next control. Preserve the
uncommitted candidate and versioned evidence under
`target/wp4m-f2-construction-proof-k64-20260819-v1`; do not start F3,
select/promote a profile, integrate production, claim Phase 4 complete, or
commit.

F1-v3 status (2026-08-19): **PASS; F2 eligible but not started**. The unchanged
F1 observability implementation was rerun once with the prospectively frozen
complete sequence `AB/AB/BA/AB/BA/AB` (pair 0 warmup, pairs 1-5 measured).
The pre-execution dry-run, raw JSONL, and preflight all match exactly. Every
semantic/custody/one-COMMIT/timer/terminal-Q gate passes; primary and
independent summaries agree; storage/schema/no-residue passes; exact Q is
`35,603 -> 37,302` (`+4.772070%`, 5/5); wall paired median is `-0.311203%`;
CPU/RSS/peak/store gates pass; and v2's passing 100-test/static and release
M4.5 proofs are hash-reused because source/executable bytes are unchanged.
F1-v1 and F1-v2 remain historical FAIL/REVISE evidence. Retain v3; do not
start F2, select/promote a profile, integrate production, claim Phase 4
complete, or commit.

F1-v2 historical status (2026-08-19): **FAIL / REVISE; F2 ineligible**. The compact-code
repair passed every measured and semantic gate: 100/100 tests and all static
checks; exact Q `35,603 -> 37,302` (`+4.772070%`, 5/5); durable wall paired
median `+0.040155%`; CPU/RSS/peak/store gates; exact identities/work and one
transaction/COMMIT; storage/schema/no-residue audit; independent recomputation;
and the release M4.5 proof (`439.551291 -> 8.668667 ms`, Q `2,222,803`). The
final protocol audit nevertheless fails because the frozen measured order was
`AB/BA/AB/BA/AB` but the retained raw rows are `BA/AB/BA/AB/BA` after the AB
warmup. No post-observation amendment or rerun is allowed. Preserve
`target/wp4m-f1-commit-io-k64-20260819-v2` as informative non-acceptance
evidence; do not advance to F2, select/promote a profile, integrate production,
claim Phase 4 complete, or commit.

F1-v1 historical status (2026-08-19): **FAIL / REVISE; F2 ineligible**. The one-variable
caller-thread observability candidate added SQLite `DBSTATUS` pager/cache
counters, exact COMMIT dispatch-to-return/reconciliation timing, and explicit
filesystem snapshots without changing schema, write shape, durability,
identities, transaction count, or COMMIT count. The retained 100-MiB full-create
five-pair overhead result was wall-neutral: control/candidate durable medians
were `936.497375 / 927.187541 ms` (`-0.994112%` arm median), paired median
`-0.975234%`, and all 5/5 pairs were within the 5% ceiling. CPU/RSS/peak and
allocated-delta median gates passed. The preregistered complete gate failed:
the longer evidence row raised exact Q from `35,603` to `38,246` bytes
(`+7.423532%`), and post-row APFS allocated DB bytes were not byte-identical
within any pair despite identical `109,268,992` logical/apparent bytes and a
favorable `-0.159750%` candidate allocated-delta median. No gate was relaxed
after observation. The smallest release M4.5 proof passed one warmup plus one
measured C0/C1 pair (`429.935542 -> 9.184333 ms`), exact identities/work,
one transaction/COMMIT, Q `2,222,803`, terminal zero, and permanent C0/C1
counters. Candidate source/executable SHA-256 are
`aeb19ba3ff4c7a01326bd55de67cdfee88048c33961b9022706be69b4a5f55ed` /
`1ac9754c8c9a72ad08aa872e29d1c78a814f3d4fa29db9581dd833c09e60f5a3`.
Raw evidence is under
`target/wp4m-f1-commit-io-k64-20260819-v1/`. Decision: preserve for revision;
do not retain as the next control, revert silently, start F2, select/promote a
profile, integrate production, or claim Phase 4 complete.

F0 status (2026-08-19): **PASS; custody freeze complete**. The accepted M4.5
state is clean commit `26f4f10122a16dd14474e93076c92f80876b798f`, tree
`0c9042da733d9ca0045a93fb69eb709f8d77ef09`, with parent-to-commit binary
patch SHA-256
`8103959f462cb073293d42ae3944ad80171cb0e9509417fc08352288e960e7d3`.
V4 remains the permanent active C0/C1 small-edit control at
`446.457042 -> 8.540708 ms` (`-98.087003%`, 5/5), Q 2,222,803 bytes and
terminal zero. V4 manifests verify 61/61 and 15/15; v3 reverifies 171/171;
the independent v4 regeneration is byte-identical; all 12 arm-copy and row
gates pass. F0 added documentation/custody only and no commit. F1 is eligible
only as a separate task; profile selection, promotion, production integration,
full-create gain, and Phase 4 completion remain false/not started.

Final checkpoint status (2026-08-19): **PASS; ready for a separate F0
freeze**. The accepted v3 terminal evidence remains preserved. The release-path
exact-capacity guard required a fresh v4 campaign, which passes at C0/C1
`446.457042 -> 8.540708 ms` (`-98.087003%`, 5/5), exact Q 2,222,803 bytes,
terminal zero, CPU -23.626% paired median, RSS -0.175% arm median, and peak
footprint +0.129% arm median. No memory extension was triggered. The H=2
regression proves four changed leaves/five changed branches, 376 covered and
14 new/different edges, 43,488-byte qualification Q, and 34 C1 versus 266,318
C0 SQL queries. All 98 tests and static gates pass. V4 measured diff is
`efc18e05d85c0ecb7a7dc02dd72205d873ad173521848800614511a7f1a1f449`;
release executable is
`7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`.
Qualification, promotion, profile selection, production integration, F0
source work, and later Phase 4 work remain not started.

Prior accepted M4.5 status (2026-08-19): **PASS** for the private K64/F64
exact-XOR same-count changed-spine milestone. The terminal v3 campaign is
`target/wp4m-m45-repair-k64-20260819-v3-terminal/`: C0/C1 durable-edit
medians are `440.023209 -> 9.134334 ms` (`-97.924124%`, 5/5 wins), exact Q is
2,222,803 bytes with terminal zero, all 96 tests and static gates pass, and
the final read-only five-lane audit has no P0/P1 blocker. The measured diff is
`e08558c030040216489365a76c0643fa83e3f49aec9425ac06b78bba4d86057d`
and the release executable is
`f84e6b0f656e03ba3c537dbce08b085c3b52094a229b6df29593082e1d745ef1`.
The historical FAIL/REVISE and v2 PASS records below remain preserved and are
superseded, not deleted. Qualification, promotion, profile selection,
production integration, and later Phase 4 work remain false/not started.
F0 may begin only as a separate next task.

Historical in-repair M4.5 status (2026-08-19): **FAIL / REVISE**. The second independent
audit withdrew the repaired PASS as an acceptance claim. The retained v2 XOR
campaign remains credible direction-only evidence, but it predates the
controlling experiment amendment and does not close exact-Q or real
COMMIT-boundary custody. F0, qualification, promotion, profile selection, and
production integration remain blocked/not started. The earlier PASS wording
and rows below are preserved as superseded historical checkpoints.

Status: M0, M2, and M3 passed their predeclared affected-metric gates; M1 and M4 were rejected and reverted; M1b throughput evidence is inconclusive but its narrow constant-factor implementation is retained. The first M4.5 checkpoint and campaign failed independent audit and remain preserved as invalid evidence. The repaired M4.5 private same-count milestone now passes correctness, authority, durability, exact-Q, byte-identical-custody, performance, CPU, external-memory, and endpoint-storage gates: exact edited FastCDC uses the predeclared `old_byte XOR 0x5a` 18,854-byte middle transform, C0/C1 durable-edit medians are `443.143 -> 9.001 ms` (`-97.969%`, 5/5 wins), and all 12 arm images match their pair base database/authority/expectation hashes. Measured implementation diff SHA-256 is `0c8d70bc6aa5944f40ead21ffefb335457df251f7df8351bef02c04acda0ac1e`; executable SHA-256 is `37643a4eb99a0ab8fcbeaa326ebb2ceada98a9716c9dbe677c6f4a53e7320d02`. Qualification=false, promotion=false, rejection=false. Branch remains `codex/empty-worktree`; no F0 or later work is included in M4.5 repair.

## Cumulative benchmark

| State | Verdict | Durable capture median | Capture MiB/s | Complete lifecycle median | Lifecycle MiB/s | Protected result |
|---|---|---:|---:|---:|---:|---|
| frozen c96 baseline | accepted starting state | 990.837 ms | 100.924736 | 1,732.868 ms | 57.707811 | reference |
| M0 measurement truth | PASS | 1,013.060 ms (+2.243%) | 98.710816 | 1,749.325 ms (+0.950%) | 57.164920 | 5/5 rows within 5% |
| M1 borrowed row reads | REJECTED, reverted | 1,012.694 ms (-0.036% vs M0) | 98.746512 | 1,738.015 ms (-0.647% vs M0) | 57.536889 | affected read metric -3.393%, 3/5 wins; gate failed |
| M2 bounded leaf reconstruction | PASS | 1,013.785 ms (+0.072% vs M0) | 98.640244 | 1,706.288 ms (-2.460% vs M0) | 58.606743 | reconstruction -7.169%, 5/5 wins; gate passed |
| M1b residual borrowed rows | INCONCLUSIVE throughput; implementation retained | 1,053.687 ms (-0.667% vs interleaved M2) | 94.904836 | 1,766.254 ms (-1.573% vs interleaved M2) | 56.617013 | residual affected sum -1.758%, 5/5 wins; 5% magnitude gate failed |
| M3 borrowed encode/ObjectId reuse | PASS for affected metric; COMMIT diagnostic unresolved | 953.829 ms (-7.057% vs interleaved M1b) | 104.840558 | 1,663.449 ms (-4.457% vs interleaved M1b) | 60.116066 | mapping/CAS -20.899%, 5/5; COMMIT +31.315%, 0/5, not independently protected |
| M4 receipt-backed same-count changed spine | REJECTED, reverted | 2.195 ms same-middle durable latency (-99.493% vs M3 control) | Unavailable for edit | 691.663 ms same-middle lifecycle (-39.024%) | Unavailable for edit | pre-COMMIT -99.965%, 5/5; maximum RSS +7.921%, protected gate failed |
| M4.5 repaired exact-CDC same-count changed spine | **PASS private milestone** | C0 443.143 -> C1 9.001 ms same-middle durable latency (-97.969%) | Unavailable for edit | 1,134.436 -> 710.947 ms same-open lifecycle (-37.329%) | Unavailable for edit | exact FastCDC/rejoin; byte-identical DB+authority custody 12/12; 5/5 wins; CPU/Q/RSS/peak/storage pass |
| M4.5 v3 terminal exact-CDC changed spine | **PASS final M4.5 milestone** | C0 440.023209 -> C1 9.134334 ms same-middle durable latency (-97.924124%) | Unavailable for edit | 1,134.875792 -> 703.763750 ms same-open lifecycle | Unavailable for edit | prospective §13.3A XOR; exact Q 2,222,803; 12/12 official and 30/30 extension arm copies; 5/5 wins; 20-pair RSS/peak adjudication pass |
| M4.5 v4 checkpoint follow-up | **PASS; F0-freeze ready** | C0 446.457042 -> C1 8.540708 ms same-middle durable latency (-98.087003%) | Unavailable for edit | 1,153.324459 -> 716.367834 ms same-open lifecycle | Unavailable for edit | exact-capacity adoption guard; direct H=2 proof; exact Q 2,222,803; 12/12 copied arms; 5/5 wins; memory extension not triggered |
| F2 bounded full-create construction proof | **FAIL / REVISE; F3 ineligible** | 929.420 -> 786.868 ms (-15.338%, 5/5) | 107.594 -> 127.086 | 1,615.793 -> 1,476.144 ms (-8.643%) | 61.889 -> 67.744 | pre-COMMIT queries -99.981%, BLOB/auth -100%; Q 55,325; CPU/RSS/storage pass; protected COMMIT +30.126%, 0/5 FAIL |
| F2-v2 standalone-authority repair | **FAIL / REVISE; historical** | 916.758 -> 652.573 ms (-28.817%, 5/5) | 109.080 -> 153.239 | 1,608.325 -> 1,343.971 ms (-16.437%) | 62.177 -> 74.406 | authority/Q/hash/cleanup PASS; historical COMMIT/tiny/relative-Q contract failed |
| F2-v3 accepted construction proof | **PASS / retain; F3 eligible separately** | 916.310 -> 659.593 ms (-28.016%, 5/5) | 109.133 -> 151.609 | 1,607.986 -> 1,353.841 ms (-15.805%) | 62.190 -> 73.864 | combined tail -67.150%, 5/5; exact pager/write/schema; absolute Q 55,325; CPU/RSS/footprint/store/tiny phases PASS |
| F3-v1 grouped INSERT + RETURNING | **FAIL / REVISE** | 674.972 -> 745.510 ms (+10.450%, 0/5) | 148.154 -> 134.136 | 1,377.332 -> 1,453.100 ms (+5.501%) | 72.604 -> 68.818 | INSERTs 5,372->103 exact; mapping +12.068%; 5,372 extra result rows; RSS/peak fail |
| F3-v2 prequery + grouped INSERT | **FAIL / REVISE** | 645.434 -> 672.161 ms (+4.141%, 0/5) | 154.935 -> 148.774 | 1,336.071 -> 1,359.882 ms (+1.782%) | 74.846 -> 73.536 | zero INSERT-return rows; 103 prequeries; wrong-kind duplicate audit P0; RSS/peak fail |
| F3-v3 optimistic ABORT grouping | **FAIL / revert; terminal** | 653.849 -> 693.111 ms (+6.005%, 0/5) | 152.941 -> 144.277 | 1,345.911 -> 1,385.098 ms (+2.912%) | 74.299 -> 72.197 | minimum 103 INSERT/0 query/0 result fast path; mapping +6.633%; RSS/peak fail; F4 ineligible |
| F3 D1-v2 causal diagnostic | **VALID / NO-GO; retain F2** | 712.158 -> 717.936 ms (+0.811%, 2/5) | 140.418 -> 139.288 | 1,432.033 -> 1,423.470 ms (-0.598% arm; +0.554% paired) | 69.831 -> 70.251 | mapping -0.380% arm/-1.005% paired, 3/5; VFS subjournal 16.127 ms; RSS/peak >5%; no >=60-ms budget; F4 ineligible |

Accepted M2 phase medians are mapping 511.358 ms, closure 391.551 ms, COMMIT 109.234 ms, reopen 1.100 ms, scrub 270.967 ms, reconstruction 421.991 ms, and ranges 0.665 ms. Both disjoint timer equations reconcile in all five rows. Against the frozen c96 baseline, accepted M2 capture is +2.316%, lifecycle is -1.534%, and reconstruction is -6.754%.

## Accepted and reverted work

Accepted M0: per-phase timer/counter snapshots; exact identity/hash/auth/write bytes; object, SQL, row/BLOB, commit, topology, W/D, Q, physical-store, CPU, and RSS evidence; explicit `Observed`/`Unavailable` handling. Implementation-only diff fingerprint: `ae6f94f90b2b088879a875fb3172bb04b9165bff76c6801490ba32d31cab3035`.

Rejected and reverted M1: a borrowed rusqlite row-value helper shared by scrub, reconstruction/pre-COMMIT streaming, and ranges, plus a borrowed mapping-payload decoder. Semantic correctness was PASS and the intended copy-elimination mechanism was PASS: it removed 314,912,120 row-to-`Vec` bytes (-99.640%) while preserving identities and counters. Throughput was nevertheless INCONCLUSIVE / GATE-FAILED: the controlling scrub+reconstruction+range median improved only 3.393% with 3/5 wins, and complete lifecycle improved only 0.646% with 3/5 wins. M1 was never a throughput PASS or profile-selection result. Attempted cumulative diff fingerprint: `12372707f4be9c8da66ef19fac2e2f187dc24ca9998a890f36c2731cf4ed98a0`. Post-revert fingerprint exactly matched accepted M0: `ae6f94f90b2b088879a875fb3172bb04b9165bff76c6801490ba32d31cab3035`.

Accepted M2: reconstruction batches each decoded leaf's at-most-K authoritative SQLite lookups through an ordinal CTE, streams returned rows, preserves order/duplicates, rejects missing/noncanonical ordering, and authenticates every canonical `ObjectId` and raw `ChunkId`. Reconstruction preparations/statements fell from 5,371 to 170 (-96.835%); total statements fell from 21,532 to 16,331. Reconstruction median fell 7.169% with 5/5 paired wins. Exact Q rose only 690 bytes for the bounded 692-byte query buffer. Accepted cumulative diff fingerprint: `56c30008e019677e8a109d1ccbc1e0282162d990b962d0f96f1ec14fae6fe59f`.

Retained M1b: one private authenticated borrowed-row callback serves only residual per-object raw paths in pre-COMMIT traversal, scrub, and ranges. The public mapping decoder stayed unchanged and M2's bounded reconstruction path was reused. Targeted copies fell by exactly 209,985,828 bytes across 10,576 borrowed rows (100% of the targeted residual bytes); total lifecycle row-copy bytes fell 66.441% to 106,064,273. A balanced interleaved ABBA comparison against the frozen M2 executable improved the affected closure+scrub+range sum 1.758% with 5/5 wins. This failed the 5% throughput magnitude gate, so throughput remains explicitly inconclusive. The code is retained separately as a deterministic constant-factor improvement because correctness/identity and intended counters passed, Q remained 33,611,532 bytes, CPU/RSS/allocated storage did not regress, and no protected phase regressed by 5%. The prior M1 +10.483% COMMIT median did not reproduce (+0.719% balanced); its small 1/5 COMMIT-win direction is recorded but not treated as causal. Retained cumulative diff fingerprint: `d9bec93db5541c3d0bdce3d01880c32b9b97939bd54d136c621fdda3d3e38e5d`.

Accepted M3: the shared canonical codec gained a borrowed-slice Bytes encoder, and the private full-create path reuses the `ObjectId` computed immediately from those exact canonical bytes. Externally supplied `(ObjectId, bytes)` pairs still take full validation; incumbent conflict/reuse rows remain read, authenticated, and byte-compared. This removed 5,370 duplicate authentication hashes over 105,291,435 bytes and the raw-payload ownership input copy while keeping semantic authentication at 421,341,279 bytes. Balanced M1b/M3 rows improved mapping/CAS 519.309 -> 410.776 ms (-20.899%) with 5/5 wins and durable capture 1,026.253 -> 953.829 ms (-7.057%) with 5/5 wins. Complete lifecycle improved only 4.457% (and about 4.006% versus frozen c96), so no >=5% whole-lifecycle claim is made. CPU, RSS, allocated store, identities, Q, SQL/BLOB/object counts, and one-COMMIT publication remained protected/non-regressive. Cumulative diff fingerprint: `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb`.

M3's COMMIT median regressed 116.511 -> 152.996 ms (+31.315%) with 0/5 wins. It is not averaged away or classified as noise. The unchanged writes, SQL/BLOB/object counts, timer boundary, and faster durable total make phase redistribution plausible, but physical I/O/fsync evidence is unavailable, so causality remains unproven. The controlling protected-file list in `../mapping/logical-persistence.md:1455-1469` does not include additive COMMIT; `../rollback/spec.md:390-424` requires it as a mandatory diagnostic. M3 therefore passes only its affected-metric gate under that explicit contract classification.

Rejected and reverted M4: a same-count-only receipt-backed changed-spine verifier authenticated the prior and replacement spines, accepted 127 equal strong edges under the authenticated prior receipt, and fully traversed the four new/different edges and one 18,867-byte new subtree. The balanced frozen comparison improved pre-COMMIT closure 430.182 ms -> 0.150 ms (-99.965%) with 5/5 wins, durable same-middle latency 433.029 ms -> 2.195 ms (-99.493%), and same-middle lifecycle 1,134.316 ms -> 691.663 ms (-39.024%). Exact identities, Q, SQL/BLOB/object equations, one COMMIT, CPU, and physical storage passed, but external maximum-RSS median rose 16,547,840 -> 17,858,560 bytes (+7.921%); peak footprint rose 11.872%. Because RSS is independently protected at 5%, M4 is **REJECTED**, not retained and not relabeled as noise. Candidate diff fingerprint: `91f394fdcfccca4c3625e7962db56ac0304f2b2b32bc65875089755316d0a139`; candidate executable SHA-256: `310d63e95a0d5dcbeedd537370c7d875cc0a2d57735e87b6254721de5a9043ad`. The isolated rollback exactly restored the accepted-M3 diff fingerprint `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb`.

Algorithm optimization achieved in the retained state: bounded database round trips per leaf replaced per-reference reconstruction statements. M1b and M3 add constant-factor copy/hash reductions only. Full create and total authentication remain Θ(source bytes + references + canonical bytes), so M3 makes no Big-O class claim; memory is O(K + largest canonical BLOB), with one borrowed SQLite row on residual paths and K=64. The rejected M4 candidate separately demonstrated same-count pre-COMMIT work changing from full-closure Θ(N) to `O(log_F(N/K) + newly divergent subtree)` with `O(active changed spine + K)` live semantic memory, but that asymptotic implementation is not present in the retained source. +1 remained bounded-spool Θ(suffix), with no logarithmic claim. No retained schema, identity, publication, public codec API churn, cache, worker, WAL, pack, source-staging, or unbounded structure changed.

## Current bottlenecks and terminal state

M2 confirms SQLite statement/setup overhead was a material reconstruction bottleneck: a 96.835% reconstruction-statement reduction produced a 7.169% reconstruction wall-time gain while byte/authentication work stayed exact. M1b confirms residual row copies were measurable but not a 5% throughput lever. M3 confirms duplicate generated-object hashes and payload ownership were material create-path costs, but durable capture remains 953.829 ms / 104.840558 MiB/s and needs another 47.580% wall reduction to reach 500 ms / 200 MiB/s. The observed M3 COMMIT increase remains a causally unresolved diagnostic; it must not be called noise or used to predict edit behavior.

M4 confirmed that authenticated prior-receipt coverage can remove nearly all redundant same-count pre-COMMIT closure replay, but its candidate failed the protected RSS gate and was reverted without an extra diagnostic campaign. Work therefore stops at accepted M3: retained diff SHA-256 `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb`, retained executable SHA-256 `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c`, unchanged HEAD `c96b5396e98db523b9a983df4ec80fdedfa971c1`. M4 cannot retroactively justify M3, predict new-file capture, or serve as profile-selection evidence; qualification, promotion, and profile rejection all remain false.

## M4.5 rolling progress

| Milestone | Status | Implementation / evidence | Release performance | Decision |
|---|---|---|---|---|
| M4.5-0 frozen evidence | PASS | M3/M4 binaries, fixture, manifest, reports, raw rows, timer/identity/COMMIT equations and current dirty diff rehashed; no source edit | NotRun | retain M3; advance to same-open witness |
| M4.5-1 authority witness | PASS after revision | `BEGIN IMMEDIATE` precedes exact in-transaction head/full scrub; move-only witness binds transaction/open/store/authority/epoch/profile/head/receipt; one issuance/use; exact-head ABA and nonzero-mode tests pass | NotRun | retain; release remains blocked on C0/C1 and accounting |
| M4.5-2 incremental proof/oracle | PASS debug activation shadow | separate disposable full-rebuild oracle reproduces frozen edited IDs; C0/C1 agree for valid, missing, multi-change, final-partial, malformed-summary and forged-mode cases; exact 4/4, 127/4 and new-chunk counters pass | NotRun | retain; release blocked on M3/M4 gates |
| M4.5-3 durability/provenance | PASS debug correctness | actual SQLite COMMIT rejection uses fresh read-only/no-DDL reconciliation; requested/prior/different/unknown, first/cleanup/dominant provenance, complete-head publication, witness invalidation and exact missing IDs pass | NotRun | retain; release remains blocked on exact accounting |
| M4.5-4 exact Q/counters | PASS debug accounting | checked scoped live-capacity sum returns q_current=0 on success/error; cache/query/execute/returned/changed SQL split; native prepare Unavailable; W/D preserved with named changed-work fields; structural paired JSON and qualification storage invariance pass | NotRun | retain; release gate now eligible for M4.5-5 preflight |
| M4.5-5 focused release A/B | PASS private same-count mechanism | one release build; C0 A/A noise, A0/C0 substrate, C0/C1 causal and A0/C1 continuity separated; C0/C1 edit median 431.490 -> 2.437 ms (-99.435%), 5/5; CPU/Q/storage pass; triggered 20-pair RSS/peak adjudication finds no repeatable >5% regression | completed | retain candidate; no qualification/promotion/production claim |
| M4.5-6 independent audit | **FAIL / REVISE** | five read-only lanes found hard exact-CDC, complete-witness-closure, transaction cleanup/provenance, exact-Q, and byte-identical-base blockers; durability/counter/evidence gaps are ranked in `milestones/m4-5/independent-audit.md`; old rows preserved as nonqualifying direction evidence | no audit-time timing | M4.5 not accepted; F0 and all post-M4.5 work blocked pending focused repair, rerun, and repeated audit |
| M4.5 repair | **PASS** | exact XOR edited-stream FastCDC and independent oracle; exact singleton witness closure; centralized transaction rollback/provenance; actual COMMIT-error reconciliation matrix; committed-result custody; 2,278,037-byte exact Q with zero on every exit; W/D Unavailable; physical byte-copied pair bases | C0 443.143 -> C1 9.001 ms (-97.969%), 5/5; CPU -24.157% paired median; RSS/peak below extension trigger; storage identical | retain repaired private candidate; qualification/promotion/profile selection remain false |
| M4.5 repaired read-only audit | **PASS** | authority/publication, exact-CDC/delta, durability/error, resource/accounting, and benchmark-custody lanes agree; terminal tracked diff `3dccd7e6...c52f`; no P0/P1 acceptance blocker | no audit-time timing | M4.5 accepted; F0 may begin only as a separate next work item |
| M4.5 second independent audit | **FAIL / REVISE — historical** | controlling XOR experiment was not prospectively amended; BEGIN ownership gap; logical Q remained post-allocation/incomplete; non-prior COMMIT tests injected state after rejected COMMIT; requested-visible diagnostic was dropped | v2 retained as direction-only evidence; fresh v3 required after repair | repair M4.5 only; F0 remained blocked at this checkpoint |
| M4.5 v3 terminal repair | **PASS** | prospective §13.3A XOR authority; pre-BEGIN admission/ownership; pre-admitted exact Q; real COMMIT dispatch-boundary reconciliation; diagnostic-carrying publication outcome; exact storage split; 96 tests | C0 440.023209 -> C1 9.134334 ms (-97.924124%), 5/5; Q 2,222,803; 20-pair RSS/peak adjudication passes | retain changed-spine mechanism; v2/preliminary-v3 remain superseded; F0 eligible only as separate work |
| M4.5 final read-only audit | **PASS — prior active** | all five lanes agree; no P0/P1 acceptance blocker; complete 171-file terminal manifest verifies; release `f84e6b0f...ef1` | no audit-time timing; accepts terminal v3 only | M4.5 accepted; no qualification/promotion/profile/production claim |
| M4.5 checkpoint-quality follow-up | **PASS — active** | §13.5A C0/C1 clarification; exact-capacity rejection; H=2 multi-ancestor/malformed proof; 98 tests; complete v4 manifest 61/61 | C0 446.457042 -> C1 8.540708 ms (-98.087003%), 5/5; Q 2,222,803; RSS/peak extension not triggered | checkpoint safe for separate F0 freeze; no F0 source work or later-phase claim |
| F0 accepted-checkpoint freeze | **PASS — active** | clean commit/tree/parent and 8103959f parent patch frozen; v4 61/61 + 15/15, v3 171/171; independent JSON byte-equal; source/spec/executable/base/command/toolchain/report hashes frozen | no new timing; retains v4 `446.457042 -> 8.540708 ms`, 5/5, Q 2,222,803 | permanent C0/C1 regression baseline; F1 eligible separately; no profile/promotion/production claim |

M4.5-0 corrected the historical interpretation without changing frozen data:
the semantic cross-process authority defect independently rejects M4; the old
max-local Q claim is not exact; “SQL preparations” were statement-cache
acquisitions/executions; and the mixed five-pair RSS result is preserved as
INCONCLUSIVE unless the new M4.5 campaign triggers the predeclared 20-pair
procedure.  Full details and artifact hashes are in
`milestones/m4-5/0-baseline-freeze.md`.

Independent audit also freezes the later causal comparison as A0 historical
M3, C0 corrected substrate with full pre-COMMIT closure, and C1 byte-identical
substrate with changed-spine qualification.  C0/C1 shadow-oracle equivalence,
complete-head ABA-safe publication, real COMMIT-error reconciliation, exact
MissingObject IDs, exact live Q, split SQL counters, and structural JSON gates
must pass before M4.5-5 can start.

## F2-v3 accepted closure

The retained F2 source/executable hashes are
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` /
`68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0`.
The implementation is the sound v2 transaction-local construction proof;
v3 changes no production byte. A prospectively frozen same-binary diagnostic
and acceptance contract repair the benchmark interpretation, not the
algorithm. Immutable v1 and v2 remain historical FAIL/REVISE evidence.

The accepted data structure is the existing bounded `FileBuilder` frontier:
one at-most-K leaf, at-most-F child and proof-summary vectors per active level,
fixed hashers/scalars, and one move-only per-put evidence. Exact proof-owned Q
is 21,952 bytes; measured total Q is 55,325 bytes and terminal zero. Full
create remains `Theta(B + N)`, live construction memory remains
`O(K + F*(H+1) + bounded buffers)`, and durable live space remains
`Theta(B_u + N)`. No metadata/schema/backend/dependency or resident linear
collection was added.

The versioned v3 root retains raw/preflight/commands/resources, environment
and validation logs, source/binaries, both analyzers, exact agreement,
storage audit, report, manifest, and final read-only audit. Final disposition
is PASS. Work stops before F3 with the branch uncommitted.

## F3 causal-diagnostic terminal closure

D1-v1 is sealed at
`target/wp4m-f3-causal-diagnostic-k64-r64-b1048576-20260820-v1` as immutable
`REVISE`. Its single started warmup-control row, invalid publication, runner
preflight history, freeze chains, terminal report, and 243-file manifest are
preserved read-only.

D1-v2 is under
`target/wp4m-f3-causal-diagnostic-k64-r64-b1048576-20260820-v2`. Exact raw
hashes are light `11d46444...56b051`, detail `c92fed42...36df6`, VFS
`40885213...62fc`, memory `269b5235...59f7e`, and M4.5
`d4b6ef55...f8c613`. The primary/independent result hashes are
`8578b0d0...d9c32` / `5f1736b9...a1a7e`; independently audited analyzer
hashes are `c584dfe5...00e78` / `e216c7e9...4637d`.

The final report is `D1-V2-FINAL-REPORT.md` in that root. It records exact
wall, CPU, instructions/cycles, Q, SQLite memory, statement cache/VM/MEMUSED,
VFS, pager/storage, schema/residue, M4.5, Amdahl, and Unavailable observations.
The result is not an acceptance benchmark for F3 code: it validates the
causal evidence and declines to authorize another candidate. Accepted F2-v3
is restored live; F4 remains ineligible; no commit is made.

Terminal custody is sealed: corrected manifest
`d1-v2-terminal-manifest-r2.tsv` has SHA-256
`f70dd3c87fcecab22fa2af8e5d6bc48cad06bf478581733ca25cfe9c66a9b905`
and verifies all 405 payloads with exact modes/bytes/hashes and no unlisted
files. The external attestation SHA-256 is
`84dc10435fdeefcc6ec4823c86f6d604a412e7c5db3036d364bcd36298fb3a61`.
The first delimiter-malformed manifest remains immutable failed-closed
history; no campaign artifact was overwritten.
