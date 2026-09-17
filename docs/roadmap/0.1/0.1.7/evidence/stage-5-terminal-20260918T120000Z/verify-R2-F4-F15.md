# Independent verification: R2-F4 / N-2 / VF-7 and R2-F15 (round-4 LOC claims)

Run against the frozen tree `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (HEAD
confirmed with `git rev-parse HEAD`; tracked tree clean per `git status --short`).
Method: the same counter the claims name — `tools/production_loc.py` at HEAD,
invoked as `python3 tools/production_loc.py --root <tree> --json` on
`git archive <rev> | tar -x -C <unique tmp dir>` extractions under `/tmp/loc-reread/`
— run independently by this verifier on every tree below. The per-commit number is
`scopes.core.lines` (the core scope's `lines` field). No repository file was
modified; writes were confined to `/tmp` and to this file. All recomputation
used committed trees via `git archive`, so it is unaffected by working-tree
state. **Line citations below refer to the committed file at `99743b2cf`**
(`git show 99743b2cf:docs/.../stage-5-report.md`); during this verification
other agents concurrently edited the working-tree copy of `stage-5-report.md`
(inserting a `de648507b` note after line 190), which shifts working-tree line
numbers below that point by +10 but does not change the committed text this
file audited.

## 1. Verdicts

| Claim | Verdict |
| --- | --- |
| R2-F4: §2 correction table lists the six recomputed rows beside the disclosed ones, accurately | **PASS** (all 12 disclosed/recomputed values reproduce; the `drift` annotation column is loose — see finding 1) |
| R2-F4: receipt `per-commit-loc-reread.log` reproduces both columns | **PASS** (all 7 rows: parents, before/after/delta, and disclosure quotes match my recomputation and the actual commit messages) |
| R2-F4: combined-to-core-only scope switch stated at `64e3f9d6a` -> `01d9f70f3` | **PASS** (both cited disclosure lines verify; boundary precision note in finding 2) |
| R2-F4: merge `a8a1ba848`'s missing disclosure line supplied (`732 -> 732`, first parent `e6ecb70d1`, docs-only) | **PASS** |
| R2-F15: §2 totals block is the counter's output on the current tree (C1 11,922 / C2 6,112 / telemetry 763 / core 18,797 / reference 65,417 / combined 84,214) | **PASS** (all six figures match exactly) |
| R2-F15: parenthetical — commits after `9327f6695` are docs/evidence only, totals did not move | **PASS** (9327f6695 tree ≡ HEAD tree on all six figures; 9327f6695..HEAD touches 20 files, all under `docs/`) |
| Falsification: `42e21677a` and `3b4941f1e` reproduce exactly | **PASS** (both match my recomputation, subtotals included) |

## 2. Commands run (repo root; exit codes)

| Command | Exit |
| --- | --- |
| `git rev-parse HEAD` → `99743b2cff2470e6634874d7ee14b9d37d0ba16e` | 0 |
| `git log --oneline -3`; `git status --short \| head -5` | 0 |
| `git log -1 --format=%B <c>` for c ∈ {07f0fe8eb, 5d08d9e83, bfd7abf2c, c17f59bef, 821ddbe30, f723663a5, a8a1ba848, 64e3f9d6a, 01d9f70f3, 42e21677a, 3b4941f1e, 6c00e0f53} | 0 |
| `git rev-parse --short=9 <c>^` for each audited commit | 0 |
| `git log --format=... 64e3f9d6a..01d9f70f3` + per-commit `grep 'Production LOC'` (scope-switch boundary) | 0 |
| `git log --format=... 64e3f9d6a~3..64e3f9d6a` + disclosures (combined check before boundary) | 0 |
| `git rev-parse a8a1ba848^2` → `579831eb6`; `git diff --stat/--name-only e6ecb70d1 a8a1ba848` | 0 |
| `git diff --name-only 9327f6695 99743b2cf` (20 files, all `docs/`); `\| grep -cv '^docs/'` printed 0 | 0 / 1 (grep found no non-docs path — expected) |
| For each rev in {4f1b7d847, 07f0fe8eb, 5d08d9e83, bfd7abf2c, c17f59bef, 979fbd5bc, 42e21677a, 821ddbe30, f723663a5, 3b4941f1e, a8a1ba848, e6ecb70d1, 9327f6695, 99743b2cf, 6c00e0f53, b069cb33a}: `rm -rf /tmp/loc-reread/<rev> && mkdir -p /tmp/loc-reread/<rev> && git archive <rev> \| tar -x -C /tmp/loc-reread/<rev>` | 0 (all 16) |
| `python3 tools/production_loc.py --root /tmp/loc-reread/<rev> --json > /tmp/loc-reread/<rev>.json` (same 16 revs) | 0 (all 16) |
| Python summary of the 16 JSONs with arithmetic self-check (C1+C2+telemetry = core; core+reference = combined) | 0 (self-check OK) |
| `grep -n .../stages-1-5-review-20260917T230700Z/per-commit-loc-stage5.log`; `wc -l` (20 rows) | 0 |

## 3. Recomputed table (this verifier's counter output)

Per-commit core totals (`scopes.core.lines`), first parent vs committed tree:

| commit | first parent (actual) | recomputed before → after (delta) | disclosed in commit message |
| --- | --- | --- | --- |
| `07f0fe8eb` | `4f1b7d847` | 11160 → 17523 (+6363) | `Production LOC: 11160 -> 17497 (delta +6337)` |
| `5d08d9e83` | `07f0fe8eb` | 17523 → 17563 (+40) | `Production LOC: 17497 -> 17525 (delta +28)` |
| `bfd7abf2c` | `5d08d9e83` | 17563 → 17698 (+135) | `Production LOC: 17525 -> 17730 (delta +205)` |
| `c17f59bef` | `bfd7abf2c` | 17698 → 17697 (−1) | `Production LOC: 17730 -> 17730 (delta 0)` |
| `821ddbe30` | `42e21677a` | 17909 → 17898 (−11) | `Production LOC: 17909 -> 17913 (delta +4)` |
| `f723663a5` | `821ddbe30` | 17898 → 17905 (+7) | `Production LOC: 17913 -> 17905 (delta -8)` |
| `a8a1ba848` (merge, PR #163) | `e6ecb70d1` | 732 → 732 (0) | no `Production LOC:` line in the message |

Full scope breakdown of every counted tree:

| rev | C1 | C2 | telemetry | core | reference | combined |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `4f1b7d847` | 4487 | 5941 | 732 | 11160 | 65417 | 76577 |
| `07f0fe8eb` | 10850 | 5941 | 732 | 17523 | 65417 | 82940 |
| `5d08d9e83` | 10890 | 5941 | 732 | 17563 | 65417 | 82980 |
| `bfd7abf2c` | 11002 | 5964 | 732 | 17698 | 65417 | 83115 |
| `c17f59bef` | 11001 | 5964 | 732 | 17697 | 65417 | 83114 |
| `979fbd5bc` | 11001 | 5964 | 732 | 17697 | 65417 | 83114 |
| `42e21677a` | 11213 | 5964 | 732 | 17909 | 65417 | 83326 |
| `821ddbe30` | 11202 | 5964 | 732 | 17898 | 65417 | 83315 |
| `f723663a5` | 11209 | 5964 | 732 | 17905 | 65417 | 83322 |
| `3b4941f1e` | 11209 | 5964 | 732 | 17905 | 65417 | 83322 |
| `a8a1ba848` | 0 | 0 | 732 | 732 | 65417 | 66149 |
| `e6ecb70d1` | 0 | 0 | 732 | 732 | 65417 | 66149 |
| `b069cb33a` | 11902 | 6043 | 763 | 18708 | 65417 | 84125 |
| `6c00e0f53` | 11908 | 6112 | 763 | 18783 | 65417 | 84200 |
| `9327f6695` | 11922 | 6112 | 763 | 18797 | 65417 | 84214 |
| `99743b2cf` (HEAD) | 11922 | 6112 | 763 | 18797 | 65417 | 84214 |

Arithmetic self-check on every row: C1+C2+telemetry = core, core+reference =
combined. (At `a8a1ba848`/`e6ecb70d1` only `layerfs-telemetry` exists under
`core/crates`, hence core = 732.)

## 4. Findings (path:line)

1. **§2 correction table — accurate.** `docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:176-183`:
   the disclosed column (`11160 -> 17497 (+6337)`, `17497 -> 17525 (+28)`,
   `17525 -> 17730 (+205)`, `17730 -> 17730 (0)`, `17909 -> 17913 (+4)`,
   `17913 -> 17905 (-8)`) matches the actual `git log -1 --format=%B` messages
   verbatim, and the recomputed column (`11160 -> 17523 (+6363)`,
   `17523 -> 17563 (+40)`, `17563 -> 17698 (+135)`, `17698 -> 17697 (-1)`,
   `17909 -> 17898 (-11)`, `17898 -> 17905 (+7)`) matches this verifier's
   independent recomputation exactly — all twelve pairs. The six disclosures
   genuinely do not reproduce, so the correction states a real defect. The
   supporting prose also checks out: the disclosed chain is internally
   self-consistent (17497 → 17525 → 17730, stage-5-report.md:186-187) and the
   maximum endpoint divergence is 38 lines (5d08d9e83/bfd7abf2c before: 17563 vs
   17525), as stated at stage-5-report.md:187-188.
2. **Receipt reproduces both columns.** `docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread.log:3-9`:
   every row's `parent=`, `before=`, `after=`, `delta=` equals my recomputation
   (including `821ddbe30 parent=42e21677a before=17909` and
   `a8a1ba848 parent=e6ecb70d1 before=732 after=732 delta=+0`), and each quoted
   disclosure line equals the actual commit message. Lines 11-15 quote
   `64e3f9d6a`'s and `01d9f70f3`'s disclosure lines verbatim. The receipt also
   reproduces the round-2 review's own
   `stages-1-5-review-20260917T230700Z/per-commit-loc-stage5.log:1-8` column for
   column on the six flagged commits, as stage-5-report.md:170-174 claims.
3. **Scope switch verified.** `64e3f9d6a`'s message discloses
   `Production LOC: 74628 -> 77136 (delta +2508)` with in-message subtotals
   `core 6152 -> 8660` and `reference 68476 -> 68476` — a **combined** headline
   (6152+68476=74628; 8660+68476=77136). `01d9f70f3`'s message discloses
   `Production LOC: 10415 -> 10893 (delta +478)` with
   `Scope: core/crates/*/src plus shipped runtime SQL under core/crates/*/sql`
   and the combined figure 78891 -> 79369 demoted to a migration subtotal — a
   **core-only** headline (3907+5776+732=10415; 4385+5776+732=10893). This is
   exactly what stage-5-report.md:193-199 states.
4. **Merge line verified.** stage-5-report.md:200-202: `a8a1ba848`'s message
   carries no `Production LOC:` line (confirmed from `%B`: only the two PR
   subject lines); its first parent is `e6ecb70d1` (confirmed); the recomputed
   first-parent comparison is 732 → 732 (delta 0, confirmed); and the change is
   documentation — `git diff --name-only e6ecb70d1 a8a1ba848` lists only
   `core/crates/layerfs-telemetry/README.md` and `USAGE.md`.
5. **R2-F15 totals verified.** stage-5-report.md:204-210 states C1 **11,922**,
   C2 **6,112**, telemetry **763**, core **18,797**, reference **65,417**,
   combined **84,214** for the tree that carries the correction. My count of
   HEAD (`99743b2cf`) is 11,922 / 6,112 / 763 / 18,797 / 65,417 / 84,214 — all
   six figures match exactly.
6. **Docs-only parenthetical verified.** `9327f6695`'s tree counts identically
   to HEAD on all six figures, and `git diff --name-only 9327f6695 99743b2cf`
   lists 20 files, every one under `docs/` (both intervening commits are
   `docs(stage5): ...`). The production totals did not move after `9327f6695`,
   as stage-5-report.md:209-210 states.
7. **Falsification samples clean.** `42e21677a` discloses `17697 -> 17909
   (delta +212)`; my recomputation of its parent `979fbd5bc` (17697) and tree
   (17909) matches, including the in-message subtotals C1 11001→11213 (+212) and
   C2 5964→5964. `3b4941f1e` discloses `17905 -> 17905 (delta 0)`; my
   recomputation of `f723663a5` (17905) and its own tree (17905) matches. No
   hidden drift in the sampled rows the review said reproduce exactly.
8. **Bonus cross-checks (beyond the required scope).** `6c00e0f53`'s disclosure
   (`18708 -> 18783 (+75)`; C1 11902→11908, C2 6043→6112, telemetry 763→763,
   combined 84125→84200) reproduces exactly against my counts of `b069cb33a`
   (18,708) and `6c00e0f53` (18,783); `b069cb33a` itself counts 18,708 /
   11,902 / 6,043 / 763, matching the round-3 end figures cited in §14/§15, so
   stage-5-report.md:637's round-4 row (core 18,708 → 18,797, +89) is
   arithmetically consistent (b069cb33a → 6c00e0f53 → 9327f6695:
   18,708 → 18,783 → 18,797).

### Minor findings (non-blocking)

1. **The `drift` annotation column (stage-5-report.md:178-183) has no single
   consistent formula.** Under recomputed−disclosed *after*-drift the rows would
   read +26 / +38 / −32 / −33 / −15 / 0; under *delta*-drift +26 / +12 / −70 /
   −1 / −15 / +15. The printed values mix the two (5d08d9e83's `+12` is only the
   delta drift; f723663a5's `-15` is only the before drift; 821ddbe30's `-15`
   fits both; bfd7abf2c prints two values `-32/-38`, the before-drift with its
   sign flipped). The substantive disclosed/recomputed columns are exact; only
   this annotation is loosely computed.
2. **Scope-switch boundary is imprecise, not wrong.** stage-5-report.md:193-195
   says "commits up to `64e3f9d6a` disclose combined...; commits from `01d9f70f3`
   disclose core-only". Both example disclosures verify, but the six commits
   *between* them (`0c548c761`, `24ef187d4`, `da8ee5769`, `b49931570`,
   `315a339fa`, `2b2dbc028`) also disclose **combined** totals (e.g. `2b2dbc028`:
   `Production LOC: 78891 -> 78891 (delta 0)`). The switch is exactly at
   `01d9f70f3` (the last combined disclosure is `2b2dbc028`, not `64e3f9d6a`), so
   the "at `64e3f9d6a` -> `01d9f70f3`" framing compresses a seven-commit window.
   Also, "each commit states its own scope in its Method line"
   (stage-5-report.md:196-197) is loose for `01d9f70f3`, whose scope is stated in
   a `Scope:` line.
3. **Receipt self-date vs §2 run date.** per-commit-loc-reread.log:1 says
   `run 2026-09-17T18:38:32Z`; stage-5-report.md:170-174 says "run 2026-09-18".
   These are the same instant in UTC vs the repo's +0800 local dates
   (18:38:32Z = 09-18 02:38:32+0800), so not a numeric discrepancy — but note
   the receipt's timestamp (09-18 02:38+0800) precedes the round-2 review
   evidence directory's name `20260917T230700Z` (= 09-18 07:07+0800) whose log
   §2 says it reproduces. File-timestamp provenance cannot be independently
   verified; every number in the receipt reproduces regardless.

## 5. UNVERIFIED

- **12 of the "other fourteen Stage-5 production commits"**
  (stage-5-report.md:185) were not independently recounted. I recounted
  `42e21677a` and `3b4941f1e` (both exact, per the task's falsification sample)
  and incidentally `6c00e0f53`/`b069cb33a` (round-3/4-era, outside the review's
  20 rows, also exact). The remaining twelve rows (`ef6bab19d`, `b262ad38c`,
  `8964db93e`, `8701eae12`, `cfc6c4ae3`, `a376acfee`, `4f2e4ca9a`, `ecec7dbe0`,
  `ab17a6958`, `b3df5461c`, `781a73661`, `eb42c1347`) rest on the review's own
  `per-commit-loc-stage5.log:9-20`, whose recomputed column matched my
  recomputation on every row I did check.
- **`64e3f9d6a`, `01d9f70f3`, `c38961f2f`, `2b2dbc028` trees were not counted.**
  Per the task, only their disclosure lines and in-message arithmetic were
  checked (all consistent); the reference 68,476 / combined 74,628-79,369
  figures were not independently reproduced.
- **§2's R16 re-derivation figures** (the `b3df5461c` column 11,772 / 6,042 /
  17,814 / 18,546, stage-5-report.md:156-157) and the §2 main table's After
  column were outside this task's claims and were not recounted (the Before
  column at `4f1b7d847` — 4,487 / 5,941 / 10,428 / 732 / 11,160 — did match my
  count exactly).
- **Timestamp provenance** of the receipt and review logs (finding 3 above).
- **No cargo commands were run** — unnecessary for LOC claims; the counter is a
  pure Python script executed from the frozen tree.
