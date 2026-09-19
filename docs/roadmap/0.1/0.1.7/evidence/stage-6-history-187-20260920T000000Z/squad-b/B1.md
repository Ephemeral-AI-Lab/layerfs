# B1 — The floor for retaining this corpus

**Squad B1 (blind half).** Every number here is **diagnostic**, not admission evidence.
Nothing under \`core/crates/\` or \`crates/\` was read for this report and nothing under either was
modified. No commit was made. The 217-row registry was not touched. \`history-stride1\` was not
run, measured or optimised.

**Claim under test (parent's, quoted):** the lane \`history-stride10\` retains the harness history
in a Store 2.65x larger than v0.1.6's for byte-identical content.

**This report does not explain the 2.65x.** It fixes the *denominator of the possible*: the
smallest size at which this content can be retained at all, and the smallest size at which it can
be retained while any version is still reachable.

---

## 0. Headline

For **371,937,306 B** of \`history-stride10\` union content (44,240 distinct oids):

| # | what it is | bytes | ratio | reachable? |
| --: | --- | --: | --: | --- |
| **L** | **lower bound** — one zstd stream over the whole union, \`-22 --ultra --long=30\` | **17,695,928** | **21.018x** | no: any read decodes 372 MB |
| **P1** | **achievable target** — 254 group frames over per-path chains, \`-19 --long=30\` | **27,184,431** | **13.682x** | yes: ≤ 11,263,931 B decoded per read (mean frame 1,464,553 B) |
| **P2** | **achievable today** — per-version delta vs the in-lane predecessor + \`zstd -19\` | **40,867,181** | **9.101x** | yes: per-object, one base hop |
| — | cross-check: **Git**, recorded control method, same 17 states | 40,238,990 | 9.243x | yes (Git's own) |
| — | per-path chains, one frame per path | 39,238,067 | 9.479x | yes: ≤ 1,241,221 B per read |
| — | no delta at all, one zstd frame per object | 114,511,800 | 3.248x | yes |
| — | **the Store as measured today** (pack bodies) | 119,894,291 | 3.102x | yes |

**The floor is 17,695,928 B = 21.018x, and it is a lower bound, not a target.** The achievable
targets are 27,184,431 B (13.682x) at ~1.5 MB access groups, or 40,867,181 B (9.101x) at strict
per-object access — the second being the only one the current architecture can express without a
new mechanism, and it is corroborated to within 1.6 % by Git's own delta store on the same states.

**The single most decision-relevant number:** the current Store's pack bodies are **6.775x** above
the lower bound and **2.934x** above P2. A store rebuilt at P2 while paying the *measured* index
and metadata overhead lands at **≈ 49.1–51.8 MB apparent** (arithmetic in §8.2) — i.e. **at or
above v0.1.6's 49,344,512 B gate**, and 4.85 % higher again in *allocated* terms (the measured
apparent→allocated factor on the current Store). **Closing the delta-base gap is necessary but
probably not sufficient to clear the gate with margin**; the group-frame arrangement is what
creates margin.

---

## 1. Instrument, identities, and what was verified before anything else

Corpus: \`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data\`, manifest SHA-256
\`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271\`, tip
\`b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed\`, 157 checkpoints.

I did **not** reuse \`shared/history_corpus.py\` as a library (it deliberately does not read blob
bytes); I read the same inputs it names — \`checkpoint-manifest.json\`, per-checkpoint
\`inputs/<sha>/manifest.tsv\` (\`mode \\t oid \\t size \\t hexpath\`, erratum E2), and the \`blobs/\`
directories — and reproduced its pins independently.

**Pin verification — all reproduced exactly on the first run, with no adjustment:**

| quantity | my reader | pin | match |
| --- | --: | --: | --- |
| stride10 states | 17 | 17 | yes |
| stride10 path-oid occurrences = \`sum(checkpoints[].files)\` | 86,064 | E1: 86,064 | yes |
| stride10 logical bytes | 561,010,345 | 561,010,345 | yes |
| **stride10 union oids / union bytes** | **44,240 / 371,937,306** | 44,240 / 371,937,306 | **yes** |
| stride3 path-oid occurrences | 259,771 | E1: 259,771 | yes |
| stride3 logical bytes | 1,676,767,835 | 1,676,767,835 | yes |
| **stride3 union oids / union bytes** | **60,000 / 583,508,923** | 60,000 / 583,508,923 | **yes** |
| stride1 union oids (reader only; the lane was never run) | 75,929 | 75,929 | yes |

The union is the **distinct blob set** of the selected states' manifests. The two stream files
built from it are each exactly 371,937,306 B (\`sha256 3caa08c8…\` oid order, \`sha256 7beab268…\`
path-version order).

Blob authentication: a sampled \`blobs/\` file hashes to its own Git object id
(\`sha1(b"blob <len>\\0" + content) == oid\`); every union oid resolves in the 157 \`inputs/*/blobs\`
directories (**0 missing**); the corpus's own \`source.git\` contains all 44,240 as \`blob\` objects
(\`git cat-file --batch-check\` → \`44240 blob\`).

**Union shape — this is what the floor is a floor *for*:**

| | stride10 | stride3 |
| --- | --: | --: |
| selected states | 17 | 53 |
| distinct paths | 16,235 | 16,520 |
| distinct oids | 44,240 | 60,000 |
| union bytes | 371,937,306 | 583,508,923 |
| path-version occurrences | 86,064 | 259,771 |
| versions after collapsing consecutive identical oids | 45,561 | 75,406 |
| of which have an in-lane predecessor | 29,326 | 58,886 |
| bytes of first versions / of delta-able versions | 92,127,730 / 285,289,945 | 92,232,674 / 604,666,334 |
| paths with exactly one version in the lane | 7,249 | 6,710 |
| cross-path duplication (occurrences ÷ distinct oids) | 1.9454 | 4.3295 |
| modes among path-versions | 85,819 × \`100644\`, 135 × \`120000\`, 110 × \`100755\` | — |

Consequence used throughout: **24.4 % of path-version bytes are first versions** (no in-lane
predecessor exists for them at all) and **75.6 % are delta-able**. That split, not the codec,
shapes every number below.

---

## 2. Method 1 — the whole union as ONE stream (the "one big blob" floor)

Two canonical orders were built, both 371,937,306 B:

* \`union_pv.bin\` — distinct oids placed at their first occurrence in **path order, then commit
  order within the path**. This is the order a store that keeps versions together would use.
* \`union_oid.bin\` — oids sorted lexicographically (order carries no locality).

\`\`\`sh
python3 streams.py                                  # writes both, from blobs/ via a 157-checkpoint oid index
zstd -L -T1 [--long=N] -c union_pv.bin | wc -c      # one frame; size read from wc -c
\`\`\`

| order | level | window | out | ratio |
| --- | --: | --- | --: | --: |
| path-version | 1 | default | 40,974,630 | 9.077x |
| path-version | 3 | default | 32,236,502 | 11.538x |
| path-version | 9 | default | 26,895,268 | 13.829x |
| path-version | 19 | default | 22,890,822 | 16.248x |
| path-version | 1 | \`--long=27\` | 29,997,565 | 12.399x |
| path-version | 3 | \`--long=27\` | 26,123,335 | 14.238x |
| path-version | 9 | \`--long=27\` | 23,043,196 | 16.143x |
| path-version | 19 | \`--long=27\` | 18,846,010 | 19.736x |
| path-version | 1 | \`--long=30\` | 29,487,028 | 12.614x |
| path-version | 3 | \`--long=30\` | 25,685,961 | 14.480x |
| path-version | 9 | \`--long=30\` | 22,655,451 | 16.417x |
| path-version | 19 | \`--long=30\` | 18,393,679 | 20.221x |
| path-version | 19 | \`--long=31\` | 18,393,679 | 20.221x |
| **path-version** | **22 \`--ultra\`** | **\`--long=30\`** | **17,695,928** | **21.018x** |
| oid-sorted | 19 | \`--long=30\` | 19,487,571 | 19.086x |
| oid-sorted | 3 | default | 101,006,092 | 3.682x |

zstd 1.5.7 (\`/opt/homebrew/bin/zstd\`), single-threaded (\`-T1\`), no checksum, no dictionary.

**This is a lower bound and it is labelled as one.** It bounds every arrangement that frames its
objects independently, because such an arrangement cannot reference bytes outside its own frame.
It is **not** an information-theoretic bound: it is one compressor's output, and a stronger model
could go lower. It is also **not reachable**: reading any one object from this stream decodes
371,937,306 B.

Residuals: at a 1 GiB window, order is worth **1,793,892 B** (19,487,571 − 18,393,679, −9.2 %).
At a 2 MiB window it is worth **68,769,590 B** (101,006,092 − 32,236,502) — with a small window
the arrangement *is* the compression.

---

## 3. Method 2 — per-path version chains

For each of the 16,235 paths: concatenate its versions in commit order (consecutive identical oids
collapsed), compress **one independent zstd frame per path**.

\`\`\`sh
python3 build_chains.py      # chains10/<i>.bin (naive) and chains10d/<i>.bin (cross-path dedup)
python3 measure_chains.py    # zstd -19 -T1 -c per file, sizes summed
\`\`\`

| arrangement | frames | raw bytes | compressed | ratio |
| --- | --: | --: | --: | --: |
| \`chains10d\` (each oid stored under its first path only) | 16,235 | 371,996,418 | **39,238,067** | **9.479x** (vs the 371,937,306 union) |
| \`chains10d\`, \`-9\` | 16,235 | 371,996,418 | 40,913,357 | 9.092x |
| \`chains10d\`, \`-19 --long=27\` | 16,235 | 371,996,418 | 39,227,555 | 9.483x |
| \`chains10\` (naive: cross-path duplicates stored again) | 16,235 | 377,417,675 | 39,847,771 | 9.471x (vs its own 377,417,675) |
| stride3, \`chains3d\`, \`-19 --long=30\` | 16,520 | 685,945,826 | **41,023,902** | **14.224x** (vs the 583,508,923 union) |

Residuals:

* Cross-path duplication costs **609,704 B** compressed (39,847,771 − 39,238,067 = +1.55 %) although
  it adds **5,421,257 B** of raw content — duplicated content is near-duplicate of its chain
  neighbours and compresses at 8.9x. Cross-path dedup is **not** a lever at chain granularity.
* Intra-chain repeats (a path reverting to earlier content inside the lane) are **59,112 B** on
  stride10 (371,996,418 − 371,937,306) and **102,436,903 B** on stride3
  (685,945,826 − 583,508,923) — a real effect at 53 states, 17.5 % of raw, and it is why the
  stride3 ratios above are quoted against the union rather than the raw chain total.
* stride3's chains compress 1.50x better per byte than stride10's (14.224x vs 9.479x against their
  unions) because 53 states give each path ~3x more versions to match against. **More retained
  states ⇒ better ratio per byte**, so stride10 is the hardest tier and the right place to iterate.

---

## 4. Method 3 — the entropy of the delta stream

For every path version after the first (29,326 of them, 285,289,945 B), a binary delta against the
immediately preceding version of the same path, using zstd's diff engine with the previous version
as a raw prefix dictionary (\`--patch-from\`, the xdelta-shaped mode of the same zstd 1.5.7 already
in the product's dependency set). The first version of each path (92,127,730 B) is compressed whole.

\`\`\`sh
python3 build_vers.py      # vers10/<path>/<version>.bin
python3 measure_vers.py    # per version: zstd -19 -T1 -c f   and   zstd -19 -T1 --patch-from=<prev> -c f
\`\`\`

| component | objects | raw bytes | compressed | ratio |
| --- | --: | --: | --: | --: |
| first versions, standalone \`-19\` | 16,235 | 92,127,730 | 30,877,035 | 2.984x |
| **delta stream**, \`--patch-from\` prev, \`-19\` | 29,326 | 285,289,945 | **9,990,146** | **28.555x** |
| **model total** | 45,561 | 377,417,675 | **40,867,181** | **9.101x** (vs the 371,937,306 union) |
| reference: every version standalone \`-19\` | 45,561 | 377,417,675 | 116,163,169 | 3.248x |
| reference: every distinct union oid standalone \`-19\` | 44,240 | 371,937,306 | 114,511,800 | 3.248x |

stride3, same instrument: first versions **30,998,222 B**, delta stream **16,907,610 B** over
604,666,334 B (**35.769x**), model total **47,905,832 B = 12.180x** of the 583,508,923 B union.

**The delta stream's entropy is 9,990,146 B for 285,289,945 B = 28.555x**, and the model is
dominated by the part that has no predecessor at all: first versions are **24.4 % of the bytes but
75.6 % of the model** (30,877,035 of 40,867,181).

Frame overhead, measured rather than assumed: an all-copy delta frame is **27 B** and a 1-byte
frame is **14 B** (\`zstd -19 --patch-from=<f> -c <f> | wc -c\` → 27; \`printf x | zstd -19 -c | wc -c\`
→ 14). So **410,564–791,802 B (4.1–7.9 %) of the 9,990,146 B delta total is framing, not payload.**

---

## 5. The bridge — ratio as a function of access granularity

Methods 1 and 2 are the two ends of one curve. I measured the middle: the same per-path chains, in
the same canonical path order, packed into **G independent frames** (so a read decodes one frame,
not the corpus).

\`\`\`sh
python3 groupcurve.py     # concatenate chains10d into G groups; zstd -19 -T1 --long=30 -c per group
\`\`\`

**stride10** (raw 371,996,418 B; ratio against the 371,937,306 B union):

| frames G | compressed | ratio | mean frame | max frame |
| --: | --: | --: | --: | --: |
| 16,235 (per path) | 39,238,067 | 9.479x | 22,910 | 1,241,221 |
| 4,059 | 34,748,100 | 10.704x | 91,647 | 9,143,491 |
| 1,015 | 30,674,214 | 12.125x | 366,499 | 9,338,312 |
| **254** | **27,184,431** | **13.682x** | **1,464,553** | **11,263,931** |
| 64 | 24,803,591 | 14.995x | 5,812,444 | 21,277,307 |
| 16 | 23,002,412 | 16.169x | 23,249,776 | 42,073,341 |
| 1 | 18,393,679 | 20.221x | 371,937,306 | 371,937,306 |

**stride3** (raw 685,945,826 B; ratio against the 583,508,923 B union):

| frames G | compressed | ratio | max frame |
| --: | --: | --: | --: |
| 16,520 (per path) | 41,023,902 | 14.224x | 1,241,221 |
| 972 | 32,144,585 | 18.153x | 28,901,610 |
| 255 | 28,752,379 | 20.294x | 32,347,621 |
| 64 | 26,377,023 | 22.122x | 40,726,552 |
| 16 | 24,562,019 | 23.757x | 93,802,270 |
| 1 | 19,628,898 | 29.727x | 583,508,923 |

**Interpretation (arithmetic, not opinion).** Cross-chain context is worth **20,844,388 B** on
stride10 (39,238,067 − 18,393,679). Of that, **12,053,636 B** is recovered at 254-frame granularity
and **16,235,655 B** at 16-frame granularity. The "21x" headline is therefore mostly a statement
about *global* near-duplication in this corpus, not about per-file redundancy.

**Framing penalty is a property of the arrangement, not of the selection.** stride10 at G=16 sits
**+25.05 %** above its one-stream figure (23,002,412 / 18,393,679); stride3 at G=16 sits **+25.13 %**
above its own (24,562,019 / 19,628,898). Two selections 1.57x apart in content, the same relative
cost for the same framing — so the curve transfers, and stride10's numbers can be read as the
17-state case of a family rather than as an accident of one selection.

---

## 6. Independent cross-check — Git's own delta store on exactly these states

The spec records Git comparators only for stride3/stride1; stride10 has none. I built one with the
**recorded control method** (\`docs/roadmap/0.1/0.1.5/issue100/git_ten_control.py\`: every object
written loose via \`pack-objects --window=0 --depth=0 --no-reuse-delta --no-reuse-object\` →
\`unpack-objects\`; commits via \`commit-tree\`; then
\`git repack -a -d -f -F --window=10 --depth=50 --threads=2\`; bare, no alternates, \`gc.auto=0\`).

\`\`\`sh
python3 githist.py      # 17 (and 53) commits from the corpus manifests; each built tree verified
                        # == the corpus's own recorded checkpoint tree oid
python3 gitexact.py     # the recorded method, stride10 and stride3
\`\`\`

| history | commits | objects | pack bytes | + idx | apparent |
| --- | --: | --: | --: | --: | --: |
| **stride10, 17 snapshot commits (new)** | 17 | 55,549 | **40,238,990** | 1,556,920 | 41,795,910 |
| **stride3, 53 snapshot commits (replication)** | 53 | 80,543 | **45,904,062** | 2,257,760 | 48,161,822 |
| *recorded* Git53 (the corpus's own control) | 53 | — | 45,912,950 | 2,257,760 | 48,951,284 |

**The method is validated on stride3 before its stride10 output is used.** My replication gives a
pack of **45,904,062 B against the recorded 45,912,950 B — a residual of −8,888 B, −0.019 %** — and
an \`.idx\` of **2,257,760 B, byte-identical to the recorded \`index_bytes\`**. All 17 built trees
equal the corpus's own recorded checkpoint \`tree\` oids (17/17; 53/53 for stride3), and every tree's
\`ls-tree -r\` count equals its manifest's line count (17/17 states, e.g. state 157: 9,415 = 9,415).

Two things follow. First, **Git's own delta store lands at 40,238,990 B for this content**, within
**1.6 %** of my per-object-delta model (40,867,181 B) — two independent compressors agreeing is the
strongest evidence in this report that ~40 MB is the honest access-respecting floor for a 17-state
selection. Second, v0.1.6's recorded stride10 Store (49,344,512 B allocated) is **1.181x** above
Git's apparent 41,795,910 B for the same states, consistent with its recorded 1.298x against Git53
(my stride3 replication: 64,024,576 / 48,161,822 = 1.329x apparent).

---

## 7. What the Store actually holds (arithmetic identity, from the Store itself)

Read-only SQL against \`/tmp/base187/sample.sqlite\` (\`python3 q.py\`, \`dbstat.py\`). This is
**Squad A's attribution territory**; it is used here only to state what the floor has to beat and to
close the byte balance exactly.

\`\`\`
pragma page_size = 4096 ; pragma page_count = 31461   ->  128,864,256 B apparent  (== recorded)
\`\`\`

| object role | code | objects | canonical bytes | with a delta base |
| --- | --: | --: | --: | --: |
| WholeFile | 1 | 44,148 | 348,460,709 | 18,344 |
| Chunk | 2 | 1,098 | 22,055,499 | 0 |
| InodeLeaf | 6 | 1,738 | 8,361,152 | 917 |
| DirectoryLeaf | 7 | 4,770 | 1,887,429 | 0 |
| ExtentLeaf / FileState / DirectoryBranch / InodeBranch / FilesystemRoot | 3,5,8,9,10 | 278 | 156,511 | 0 |
| **total** | | **52,032** | **380,921,300** | 19,261 |

Two exact identities fall out, and both matter for the floor:

1. **Content = role 1 + role 2 = 370,516,208 B = 0.99618x the 371,937,306 B whole-file union.** The
   residual is **1,421,098 B (0.382 %)**, explained by chunk-level dedup of large files (the union
   counts whole files, the Store counts their chunks). **The Store does not store the content
   twice.** Whatever produces the 2.65x, it is not duplicate storage.
2. **Metadata = roles 3,5,6,7,8,9,10 = 10,405,092 B = 2.732 % of canonical.** Roles
   2+3+5+6+7+8+9+10 = **7,884 objects / 32,460,591 B**, reproducing the bucket table's "other
   lanes" aggregate exactly on both figures — which also proves that figure *contains* the
   22,055,499 B of chunk content and cannot be read as metadata alone.

SQLite overhead above pack bodies, measured with \`dbstat\` (\`128,749,568\` allocated page bytes
+ 28 free pages × 4096 = 114,688 → exactly 128,864,256):

| region | bytes |
| --- | --: |
| \`object_packs\` (the pack bodies themselves) | 120,983,552 |
| \`objects\` (object index) | 3,514,368 |
| \`objects_locations\` (index) | 2,547,712 |
| \`objects_bases\` (index) | 1,576,960 |
| \`metadata_value_groups\` + its autoindex | 118,784 |
| \`store_policy\` + \`sqlite_schema\` | 8,192 |
| free pages | 114,688 |
| **total apparent** | **128,864,256** |

Metadata measured from the corpus side, independently of the Store: the 17 selected states'
`manifest.tsv` files concatenate to **14,284,614 B** and compress to **1,212,430 B** at `zstd -19`
(**11.78x**); the 17 oracle JSONs are 20,643,150 B → 1,770,340 B. The Store's own metadata is
10,405,092 B canonical, stored inside the other-lanes 8,953,237 B.

**Overhead above the 119,894,291 B of pack bodies = 8,969,965 B (7.48 % of apparent).** Of that, the
per-object index is **7,639,040 B for 52,032 objects = 146.8 B/object** — and that already contains
the 32-byte base reference of each of the 19,261 delta'd objects (the \`objects_bases\` index; the
Store's \`object_id\` is 32 bytes, verified by \`length(object_id)\`). Any per-object-indexed design
pays this, and it is the reason the *store-level* floor is not the *content-level* floor.

---

## 8. The floor, and what it costs at store level

### 8.1 Content floor (the deliverable's number)

> **17,695,928 B for 371,937,306 B of union content = 21.018x.**
> Method: one zstd 1.5.7 stream, \`-22 --ultra --long=30\` (1 GiB window), over the 44,240 distinct
> contents in path-then-commit order.
> **This is a lower bound**, valid for any arrangement that frames independently and reachable by no
> arrangement that must read one version at a time.

> **Achievable target: 27,184,431 B = 13.682x** — 254 frames of ≤ 11,263,931 B (mean 1,464,553 B),
> \`-19 --long=30\`, over per-path chains in canonical path order.

> **Achievable today: 40,867,181 B = 9.101x** — per-version delta against the in-lane predecessor +
> \`zstd -19\`; corroborated by Git's 40,238,990 B pack on the same states (1.6 % apart).

### 8.2 Store-level floor, using the overhead actually measured in §7

The Store is one SQLite file, so the content floor is not the file size. Two computations bracket
it; both use measured components, and both assumptions are labelled.

**(a) Optimistic — scale the whole pack-body region by the content ratio**
\`\`\`
pack bodies 119,894,291 × (40,867,181 / 119,894,291)      40,867,181
+ pack-page slack, scaled (1,089,261 × 0.3409)               371,437
+ object index, unchanged shape (52,032 objects)           7,639,040
+ metadata_value_groups + policy/schema                      126,976
+ free pages                                                 114,688
---------------------------------------------------------------------
  apparent floor                                       ≈  49,119,322 B
\`\`\`
**0.995x of the 49,344,512 B gate** — i.e. it does not clear it. And (a) is optimistic because it
shrinks the metadata *content* along with the content, which is wrong.

**(b) Honest — content at the floor, metadata and index held at their measured cost**
\`\`\`
content roles 1+2 at the P2 ratio (370,516,208 × 40,867,181 / 371,937,306)  ≈ 40,716,000
+ metadata roles 3,5,6,7,8,9,10, stored as today
  (10,405,092 canonical at the other-lanes 3.63x)                           ≈  2,877,000
+ object index (unchanged, 52,032 objects)                                      7,639,040
+ pack-page slack, scaled                                                          369,700
+ metadata_value_groups + policy/schema                                            126,976
+ free pages                                                                       114,688
------------------------------------------------------------------------------------------
  apparent floor                                                          ≈ 51,843,404 B
\`\`\`
**1.051x of the gate — above it.** In *allocated* terms both are ~4.85 % higher again (the measured
apparent→allocated factor on the current Store: 135,118,848 / 128,864,256), i.e. **≈ 51.5–54.4 MB**
against a 49,344,512 B allocated gate.

**Assumptions, labelled:** (a)/(b) hold the object index at its present shape and per-object cost,
and (b) holds metadata at today's other-lanes compression. Neither was measured against a rebuilt
store — no product change was made, and none is proposed here.

**Why this matters more than the floor itself.** Both routes land at **≈ 49–52 MB apparent**, i.e.
**at or above the gate**, because **7.64 MB of per-object index and ~2.9 MB of metadata do not
shrink when the content does**. The P1 arrangement (27.2 MB content) is the only one of the three
whose content floor is below the gate by more than the Store's own fixed overhead.

---

## 9. Practical constraints (stated, not silently ignored)

* **Random access must remain possible.** The arrangements cost different amounts of *decoded bytes
  per read*: P2 → one base + one delta (≤ 1,241,221 B worst observed); per-path chain →
  ≤ 1,241,221 B; 254-group → ≤ 11,263,931 B; one stream → 371,937,306 B. I report the **byte** cost
  of access and never a time: this machine was shared with other diagnostic work throughout, so
  **every timing reading would be invalid and none is reported**.
* **One SQLite file.** Pack bodies live in \`object_packs\` as BLOBs; the measured page slack is
  1,089,261 B and the measured index is 7,639,040 B. All three arrangements fit the same container;
  none needs a sidecar.
* **The existing product must be able to produce it.** Only **P2** maps onto a mechanism that exists
  today: the advisory predecessor route (\`cas/save.rs:100\` → \`encoding/delta/select.rs:342\`) fed by
  \`file/edit/apply.rs:121\`. P1 (group frames) and L need a new *arrangement* of the pack, not a new
  codec. This report makes no claim about what the product's own delta codec achieves; P2 is a
  floor for that arrangement, measured with a full-strength delta engine.
* **999-line ceiling and no-new-dependency.** Every measurement above uses zstd 1.5.7, which the
  product already links (pack codec \`1 = Zstandard\`). Nothing here requires a new dependency. Any
  *proposal* built on §5's curve must fit the 999-physical-line rule and the 200-line
  \`lib.rs\`/\`mod.rs\` rule; the floor model itself is Python and is explicitly not shippable.
* **What the floor is a floor *for*.** The union is *content*: 44,240 blobs. The Store must also
  carry the 17 states' trees, inodes, modes and path mappings — **10,405,092 B canonical**, measured
  today (§7). §8.2 counts it; §0's content numbers do not.

---

## 10. Confirmation at \`history-stride3\` (the second gate)

Same instrument, same commands, selection \`range(1,158,3)\` (53 states), union **583,508,923 B over
60,000 oids** (verified). All ratios are against that union.

| method | bytes | ratio |
| --- | --: | --: |
| one stream, \`-22 --ultra --long=30\` | **18,884,226** | **30.900x** |
| one stream, \`-19 --long=30\` | 19,628,898 | 29.727x |
| group frames G=16 | 24,562,019 | 23.757x |
| group frames G=64 | 26,377,023 | 22.122x |
| group frames G=256 (255 frames) | 28,752,379 | 20.294x |
| group frames G=1024 (972 frames) | 32,144,585 | 18.153x |
| per-path chains (16,520 frames), \`-19 --long=30\` | 41,023,902 | 14.224x |
| per-version delta model (30,998,222 + 16,907,610) | 47,905,832 | 12.180x |
| Git, recorded control method, 53 commits | 45,904,062 | 12.711x |

Scaling, which is the point of the confirmation: **the same method that gives 21.018x on 17 states
gives 30.900x on 53 states** for 1.569x the content. The one-stream floor grows only **1.067x**
(17,695,928 → 18,884,226) while the content grows 1.569x; the delta model grows 1.172x
(40,867,181 → 47,905,832). The per-byte floor *improves* as more states are retained, so stride10 is
the hardest tier — as the tier policy assumes. Nothing here changes the instruction never to
optimise stride1.

---

## 11. Hypotheses (explicitly labelled — not measurements)

* **H1.** The 20,844,388 B of cross-chain context the one-stream arrangement exploits is mostly
  *near-duplication between different paths* (vendored copies, moved files, generated files) rather
  than shared boilerplate inside files. **Not verified**: I did not identify which region pairs zstd
  matched, and no per-pair attribution was attempted.
* **H2.** A group-frame arrangement at ~1.5 MB frames (27,184,431 B, 13.682x) is producible by the
  existing product by changing pack *grouping* only — same zstd, same delta codec, no new
  dependency, and it must fit the 999-line rule. **Not implemented, not measured.**
* **H3.** The gap between today's Store (119,894,291 B of pack bodies) and P2 (40,867,181 B) is
  dominated by missing delta bases rather than by codec choice, because the current per-object
  baseline (114,511,800 B at \`-19\`, 3.248x) is already within 3 % of the Store's whole-file lane
  (110,941,054 B stored for 348,460,709 B canonical = 3.141x). **This is arithmetic on given numbers
  plus one measurement of mine; the attribution itself is Squad A's.**
* **H4.** A stronger general-purpose compressor than zstd (a long-range CM/PPM model, or a trained
  dictionary shared across frames) would beat 17,695,928 B on the one-stream arrangement and would
  also move the group-frame curve. **Not measured**; no such tool was run, and this report makes no
  claim about true information-theoretic entropy.

---

## 12. Negative results (what was ruled out, how, with what number)

* **N1 — "a better zstd level fixes it" is ruled out.** At the default window, level 19 buys only
  16.248x (22,890,822 B); the *window* is the lever, not the level: \`-19 --long=30\` = 18,393,679 B,
  \`-22 --ultra --long=30\` = 17,695,928 B. Conversely a small window with no ordering locality
  (\`-3\`, oid order) gives **101,006,092 B = 3.682x**, which is where the Store already is (3.102x).
  Codec tuning cannot move this.
* **N2 — a 2 GiB window buys nothing over 1 GiB.** \`--long=31\` and \`--long=30\` both give
  **18,393,679 B**, byte-identical: once the window exceeds the 371,937,306 B corpus, more window is
  free of value. The requirement is "window ≥ corpus", not "as much window as possible".
* **N3 — the bare-oid Git pack is NOT the recorded comparator, and using it would have overstated
  the floor by 2.17x.** Feeding the 44,240 oids to \`git pack-objects\` (no path names ⇒ no
  name-hash delta grouping) gives **87,181,614 B** (window 10) / **70,739,157 B** (window 250),
  versus **40,238,990 B** with the recorded method. Retained because it quantifies how much the
  *method*, not the content, decides this comparison — and because it is the trap this measurement
  could most easily have fallen into.
* **N4 — "delta against the immediate predecessor" is not the best arrangement.** It gives
  40,867,181 B against per-path chains' 39,238,067 B: **4.0 % worse**, because both are dominated by
  the 92,127,730 B of first versions that have no predecessor at all. Choosing deltas over chains is
  a decision about *access semantics*, not about size. (At 53 states the ordering reverses:
  47,905,832 B delta model vs 41,023,902 B chains — chains win by 14.4 % there.)
* **N5 — cross-path deduplication is not a lever.** Naive chains (storing shared content once per
  path) cost only **609,704 B** more than the deduplicated arrangement (+1.55 %), despite 5,421,257 B
  more raw content.
* **N6 — duplicate content storage does not explain the 2.65x.** The Store's content objects
  (role 1 + role 2 = 370,516,208 B) are **0.99618x** the union (371,937,306 B); chunk-level dedup
  makes it *smaller*, not larger. The Store holds the content once.
* **N7 — the "other lanes" figure cannot be read as metadata.** 7,884 objects / 32,460,591 B
  canonical is exactly reproduced by roles 2+3+5+6+7+8+9+10, i.e. it *contains* the 22,055,499 B of
  chunk content. Reading it as "metadata overhead" would overstate metadata by ~2.1x.
* **N8 — the corpus reader's own facts were not re-derived and did not need to be.** All stride10
  pins and both stride3 pins reproduced on the first run with no adjustment; the E1 path-state
  numbers (86,064 / 259,771) reproduce as path-oid occurrence counts, independently confirming that
  reading.
* **N9 — the framing penalty is not a stride10 artifact.** stride10 G=16 is +25.05 % over its
  one-stream figure and stride3 G=16 is +25.13 % over its own; the two selections differ by 1.57x in
  content but not in the relative cost of framing.

---

## 13. Reproduction

\`\`\`sh
# 0. tools
which zstd && zstd --version            # /opt/homebrew/bin/zstd, 1.5.7

# 1. index + union (reproduces every pin; prints both totals)
python3 build_index.py                  # 44,240 / 371,937,306  and  60,000 / 583,508,923
python3 e1.py                           # 86,064 / 259,771 occurrences; logical bytes
python3 chains.py                       # 45,561 versions, 29,326 delta-able
python3 streams.py                      # union_oid.bin, union_pv.bin (both 371,937,306)

# 2. method 1  (full level/window matrix -> bigblob.log, bigblobB.log)
jobA.sh ; jobB.sh

# 3. method 2
python3 build_chains.py && python3 measure_chains.py
python3 build3.py       && python3 measure_chains3.py

# 4. method 3
python3 build_vers.py   && python3 measure_vers.py
python3 measure_vers3.py

# 5. the access-granularity curve
python3 groupcurve.py ; python3 groupcurve3.py

# 6. baselines and cross-checks
python3 objstand.py                     # per-object -19: 114,511,800
python3 meta.py                         # metadata streams: 14,284,614 -> 1,212,430 (zstd -19)
python3 githist.py && python3 gitexact.py   # recorded Git control method, validated on stride3

# 7. the Store (read-only)
python3 q.py ; python3 dbstat.py        # roles, canonical totals, page accounting
\`\`\`

All artifacts are under \`/tmp/b1floor/\` (scripts, \`.bin\` streams, \`chains*/\`, \`vers*/\`, \`git*.git\`,
logs). \`LAYERFS_CONSTRUCTION_WORKERS\` was never set and no product lane was run for this report;
\`/tmp/base187/sample.sqlite\` was only read. Report written to
\`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-b/B1.md\`.

## 14. What this does not establish

* It is **not** an explanation of the 2.65x and it attributes nothing to the product's code paths.
  §7's composition is an arithmetic identity on the Store, not a causal claim.
* It is **not** admission evidence, and none of it is a gate result.
* It is **not** an information-theoretic entropy. §2's number is one compressor's output.
* It does **not** propose a change to \`core/crates/\`; H2 is a hypothesis with its constraints stated.
* No timing was measured or reported.
