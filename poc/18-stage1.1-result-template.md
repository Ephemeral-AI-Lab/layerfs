# Stage 1.1 Result and Handoff Template

Status: **controlling output contract for Stage 1.1**
Authority: used only with
[16 — Stage 1.1 specification](16-stage1-part1-apple-edge-benchmark.md)
Purpose: freeze the exact result files, field names, section order, units,
tables, availability handling and final agent response before implementation
or measurement

## 0. Non-negotiable reporting rules

```text
raw rows are append-only
all 47 scheduled rows are retained
all 51 edit/sub-edit operations are retained
failed rows are never deleted or replaced
integer nanoseconds are the machine timing authority
integer bytes are the machine size authority
human milliseconds/MiB/s are derived only from machine integers
Unavailable is never encoded as zero
NotApplicable is never encoded as zero work observed
patch, accepted-splice shift, and FullFallback populations are never combined
physical-to-logical and logical-to-physical populations are never combined
oracle work is outside operation latency but inside row and complete wall
summary tables are derived only from rows.jsonl
summary status never promotes a failed hard gate
```

Allowed terminal dispositions:

```text
PASS
REVISE
FAIL
```

Allowed observation availability:

```text
Observed
Unavailable
NotApplicable
```

Machine artifacts use:

```text
time          integer nanoseconds
size          integer bytes
count         integer
ratio         JSON number derived from exact integers
identity      lowercase hexadecimal string
status        exact enum string
```

Human tables display:

```text
milliseconds       six decimal places below 1 ms; three otherwise
MiB/s              three decimal places
MiB                three decimal places
ratios             three decimal places
Unavailable        literal `Unavailable`, never `0`
NotApplicable      literal `N/A`, never `0`
```

## 1. Required artifact tree

```text
target/layerfs-stage1-apple-edge-<timestamp>/
├── environment.json
├── master.json
├── readiness.json
├── schedule.json
├── rows.jsonl
├── summary.json
├── summary.md
├── campaign-time.txt
└── stderr.txt              only when nonempty failure output exists
```

Required artifact custody table in both `summary.md` and `summary.json`:

| Artifact | Required identity |
|---|---|
| `environment.json` | SHA-256 |
| `master.json` | SHA-256 |
| `readiness.json` | SHA-256 and exact admitted external-receipt equality |
| `schedule.json` | SHA-256 |
| `rows.jsonl` | SHA-256, line count and valid-row count |
| `summary.json` | SHA-256 recorded in terminal handoff after close |
| `summary.md` | SHA-256 recorded in terminal handoff after close |
| `campaign-time.txt` | SHA-256 |
| release executable | SHA-256 and BLAKE3 |
| Rust/Cargo source tree | BLAKE3 plus per-file manifest digest |

`summary.json` and `summary.md` cannot self-bind their final digest. The final
handoff response records those two digests after both files are closed.

## 2. `summary.md` exact section order

The generated human report must use these headings in this order. Sections
may not be omitted. An unavailable table remains present with literal
`Unavailable` cells and an explanation below it.

```text
# LayerFS Stage 1.1 — Single-file APFS Edge Result

## 1. Disposition and custody
## 2. Overall gate scoreboard
## 3. Physical APFS edit to LayerFS checkpoint
## 4. Physical count-changing amplification
## 5. Logical LayerFS edit to physical APFS refresh
## 6. Refresh-route summary
## 7. Canonical locality
## 8. Multi-edit save bursts
## 9. Fresh Verified history sessions
## 10. Materialization and reconstruction
## 11. Transaction and authentication closure
## 12. Storage growth and amplification
## 13. Resource closure
## 14. Timer closure
## 15. Preserved failures and unavailable observations
## 16. Final disposition
```

## 3. `summary.md` copy template

### 3.1 Disposition and custody

```markdown
# LayerFS Stage 1.1 — Single-file APFS Edge Result

Disposition: `{{PASS|REVISE|FAIL}}`

## 1. Disposition and custody

| Field | Value |
|---|---:|
| Run directory | `{{absolute run directory}}` |
| Git commit | `{{40 lowercase hex}}` |
| Dirty tree | `{{false|true}}` |
| Source BLAKE3 | `{{64 lowercase hex}}` |
| Source manifest SHA-256 | `{{64 lowercase hex}}` |
| Executable SHA-256 | `{{64 lowercase hex}}` |
| Executable BLAKE3 | `{{64 lowercase hex}}` |
| Fixture BLAKE3 | `{{64 lowercase hex}}` |
| APFS identity | `{{recorded volume identity}}` |
| StoreId | `{{64 lowercase hex}}` |
| Store profile | `page=4096; cache=1280; spill=1280; DELETE/FULL/FILE/mmap=0` |
| Measured workflows | `1 / 1` |
| Valid rows | `{{n}} / 47` |
| Edit/sub-edit operations | `{{n}} / 51` |
| Durable transitions | `{{n}} / 34` |
| Initial root | `R0={{root}}` |
| Terminal root | `R34={{root}}` |
| Initial bytes | `25,165,824` |
| Maximum bytes | `{{observed; required 25,227,264}}` |
| Terminal bytes | `{{observed; required 25,165,824}}` |
| Complete workflow wall | `{{milliseconds}} ms` |

| Artifact | SHA-256 | Additional identity |
|---|---|---|
| `environment.json` | `{{digest}}` | — |
| `master.json` | `{{digest}}` | fixture BLAKE3 `{{digest}}` |
| `readiness.json` | `{{digest}}` | admitted receipt `{{exact-match status}}` |
| `schedule.json` | `{{digest}}` | `47 rows / 51 edit-suboperations / 34 transitions` |
| `rows.jsonl` | `{{digest}}` | `{{line count}} lines / {{valid rows}} valid` |
| `campaign-time.txt` | `{{digest}}` | timer equation `{{status}}` |
| release executable | `{{digest}}` | BLAKE3 `{{digest}}` |
| Rust/Cargo source tree | manifest SHA-256 `{{digest}}` | BLAKE3 `{{digest}}` |
```

### 3.2 Overall gate scoreboard

```markdown
## 2. Overall gate scoreboard

| Gate | Required | Observed | Status |
|---|---:|---:|---|
| Rows | `47` | `{{n}}` | `{{status}}` |
| Edit/sub-edit operations | `51` | `{{n}}` | `{{status}}` |
| Durable transitions | `34` | `{{n}}` | `{{status}}` |
| Complete workflow | `<60,000 ms` | `{{ms}}` | `{{status}}` |
| Physical oracles | `51 exact` | `{{n}} exact` | `{{status}}` |
| Canonical transition oracles | `34 exact` | `{{n}} exact` | `{{status}}` |
| Save bursts | `4 exact` | `{{n}} exact` | `{{status}}` |
| Selected historical roots | `8 exact` | `{{n}} exact` | `{{status}}` |
| Route labels | exact | `{{counts}}` | `{{status}}` |
| Live rematerializations | `0` | `{{n}}` | `{{status}}` |
| RSS peak | `<=33,554,432 B` | `{{bytes}}` | `{{status}}` |
| Q high-water | `<=8,388,608 B` | `{{bytes}}` | `{{status}}` |
| Q terminal after every operation | `0` | `{{max}}` | `{{status}}` |
| FD baseline/terminal | equal | `{{n}} / {{n}}` | `{{status}}` |
| Store connections terminal | `0` | `{{n}}` | `{{status}}` |
| Owned residue | `0` | `{{n}}` | `{{status}}` |
| Network | `0` | `{{n}}` | `{{status}}` |
```

### 3.3 Physical edit to checkpoint

```markdown
## 3. Physical APFS edit to LayerFS checkpoint

| Operation | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms | Oracle | Status |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Overwrite | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` | `{{status}}` |
| Insert | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` | `{{status}}` |
| Delete | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` | `{{status}}` |
| Append | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` | `{{status}}` |
| Truncate | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` | `{{status}}` |
| **All** | **15** | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/15` | `{{status}}` |

| Size band | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| Near 8 KiB | 5 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` |
| Near 16 KiB | 5 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` |
| Near 32 KiB | 5 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` |
```

### 3.4 Count-changing amplification

```markdown
## 4. Physical count-changing amplification

| Seq | Operation | Offset | Suffix B | Replacement B | Native read B | Native write B | Equation | Route | Status |
|---:|---|---:|---:|---:|---:|---:|---|---|---|
| `{{seq}}` | `{{kind}}` | `{{offset}}` | `{{S}}` | `{{B}}` | `{{read}}` | `{{write}}` | `read=S; write=S+B` | `{{route}}` | `{{status}}` |

| Kind | n | Suffix shifted B | Native read B | Native write B | Amplification |
|---|---:|---:|---:|---:|---:|
| Insert | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{ratio}}` |
| Delete | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{ratio}}` |
| Append | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{ratio}}` |
| Truncate | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{ratio}}` |
```

The generated table contains all twelve count-changing physical rows; the one
placeholder row above establishes the exact column order.

### 3.5 Logical edit to physical refresh

```markdown
## 5. Logical LayerFS edit to physical APFS refresh

| Operation | n | Logical p50 ms | Logical p95 ms | Route class | Refresh p50 ms | Refresh p95 ms | End-to-end p50 ms | End-to-end p95 ms | Oracle |
|---|---:|---:|---:|---|---:|---:|---:|---:|---|
| Overwrite | 3 | `{{v}}` | `{{v}}` | Patch | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` |
| Insert | 3 | `{{v}}` | `{{v}}` | Shift | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` |
| Delete | 3 | `{{v}}` | `{{v}}` | Shift | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` |
| Append | 3 | `{{v}}` | `{{v}}` | Shift | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` |
| Truncate | 3 | `{{v}}` | `{{v}}` | Shift | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | `{{n}}/3` |
```

### 3.6 Refresh routes

```markdown
## 6. Refresh-route summary

| Route | Required count | Observed | p50 ms | p95 ms | Physical B | Rematerializations | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| ClonePatch | `0..3` | `{{n}}` | `{{v|N/A}}` | `{{v|N/A}}` | `{{B}}` | `0` | `{{status}}` |
| InPlacePatch | `0..3` | `{{n}}` | `{{v|N/A}}` | `{{v|N/A}}` | `{{B}}` | `0` | `{{status}}` |
| Patch aggregate | `3` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| CloneShift | `0..12` | `{{n}}` | `{{v|N/A}}` | `{{v|N/A}}` | `{{B}}` | `0` | `{{status}}` |
| InPlaceShift | `0..12` | `{{n}}` | `{{v|N/A}}` | `{{v|N/A}}` | `{{B}}` | `0` | `{{status}}` |
| Shift aggregate | `12` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| Insert Shift | `3` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| Delete Shift | `3` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| Append Shift | `3` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| Truncate Shift | `3` | `{{n}}` | `{{v}}` | `{{v}}` | `{{B}}` | `0` | `{{status}}` |
| FullFallback | `0` | `{{n}}` | `{{v|N/A}}` | `{{v|N/A}}` | `{{B}}` | `0` | `{{status}}` |
```

### 3.7 Canonical locality

```markdown
## 7. Canonical locality

| Population | Transitions | CDC expected B | CDC observed B | Unaffected reads B | Unaffected writes B | Max nodes read | Max nodes emitted | Status |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| Physical checkpoints | 15 | `{{B}}` | `{{B}}` | `0` | `0` | `{{n}}` | `{{n}}` | `{{status}}` |
| Direct logical edits | 15 | `{{B}}` | `{{B}}` | `0` | `0` | `{{n}}` | `{{n}}` | `{{status}}` |
| Save bursts | 4 | `151,552` | `{{B}}` | `0` | `0` | `{{n}}` | `{{n}}` | `{{status}}` |
| **Total** | **34** | `{{B}}` | `{{B}}` | **0** | **0** | `{{n}}` | `{{n}}` | `{{status}}` |
```

### 3.8 Save bursts

```markdown
## 8. Multi-edit save bursts

| Root | Pattern | Sub-edits | Native ms | Oracle ms | Checkpoint ms | Row ms | Transactions | COMMITs | Final B | Status |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| R31 | Autosave hotspot | 8 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | 1 | 1 | 25,165,824 | `{{status}}` |
| R32 | Insertion-boundary | 3 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | 1 | 1 | 25,169,920 | `{{status}}` |
| R33 | Append/rotation | 4 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | 1 | 1 | 25,165,824 | `{{status}}` |
| R34 | Alternating distant | 6 | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | 1 | 1 | 25,165,824 | `{{status}}` |
| **Total** | — | **21** | `{{v}}` | `{{v}}` | `{{v}}` | `{{v}}` | **4** | **4** | — | `{{status}}` |
```

### 3.9 Verified history

```markdown
## 9. Fresh Verified history sessions

| Session | Head | Roots checked | Open/scrub ms | Objects authenticated | Bytes authenticated | Probe B | Writer tx | Native writes | Status |
|---:|---|---|---:|---:|---:|---:|---:|---:|---|
| 1 | R5 | R0,R5 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |
| 2 | R10 | R0,R5,R10 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |
| 3 | R15 | R0,R5,R10,R15 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |
| 4 | R20 | R0,R15,R20 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |
| 5 | R25 | R0,R15,R20,R25 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |
| 6 | R30 | R0,R15,R20,R25,R30 | `{{v}}` | `{{n}}` | `{{B}}` | `{{B}}` | 0 | 0 | `{{status}}` |

| Probe ordinal | n | p50 ms | p95 ms | Non-payload rows | Payload rows | Cache classification |
|---:|---:|---:|---:|---:|---:|---|
| 1 | 21 | `{{v}}` | `{{v}}` | `{{n}}` | `{{n}}` | first root/path resolution |
| 2 | 21 | `{{v}}` | `{{v}}` | `{{n}}` | `{{n}}` | exact root/path plan hit |
| 3 | 21 | `{{v}}` | `{{v}}` | `{{n}}` | `{{n}}` | exact root/path plan hit |
```

### 3.10 Materialization

```markdown
## 10. Materialization and reconstruction

| Root | Purpose | Logical B | Wall ms | MiB/s | Native write B | Exact bytes | Metadata | Cleanup |
|---:|---|---:|---:|---:|---:|---|---|---|
| R0 | Initial cold managed | 25,165,824 | `{{v}}` | `{{v}}` | `{{B}}` | `{{status}}` | `{{status}}` | retained live |
| R15 | Physical-chain milestone | 25,165,824 | `{{v}}` | `{{v}}` | `{{B}}` | `{{status}}` | `{{status}}` | `{{status}}` |
| R30 | Logical-refresh milestone | 25,165,824 | `{{v}}` | `{{v}}` | `{{B}}` | `{{status}}` | `{{status}}` | `{{status}}` |
| R34 | Burst-chain milestone | 25,165,824 | `{{v}}` | `{{v}}` | `{{B}}` | `{{status}}` | `{{status}}` | `{{status}}` |
```

### 3.11 Transaction/authentication closure

```markdown
## 11. Transaction and authentication closure

| Equation | Required | Observed/failures | Status |
|---|---:|---:|---|
| Generation increment | `34/34` | `{{n}}/{{failures}}` | `{{status}}` |
| Writer transactions | `34` | `{{n}}` | `{{status}}` |
| Committed transactions | `34` | `{{n}}` | `{{status}}` |
| Rolled-back transactions | `0` | `{{n}}` | `{{status}}` |
| Publication COMMITs | `34` | `{{n}}` | `{{status}}` |
| fetched = authentication | every applicable row | `{{failures}} failures` | `{{status}}` |
| fetched = role decode | every applicable row | `{{failures}} failures` | `{{status}}` |
| new auth = created + reused | every publication | `{{failures}} failures` | `{{status}}` |
| incumbent auth = reused | every publication | `{{failures}} failures` | `{{status}}` |
| Payload batch maximum | `<=64` | `{{n}}` | `{{status}}` |

| Counter phase | Rows | Statements | Fetched/auth/role | Object read B | Object write B | Tx/COMMIT | Scrubs | Engine/VFS scratch tables | Q high B | Connections |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `{{phase}}` | `{{n}}` | `{{n}}` | `{{n}}/{{n}}/{{n}}` | `{{B}}` | `{{B}}` | `{{n}}/{{n}}` | `{{n}}` | `{{n}}/{{n}}` | `{{B}}` | `{{n}}` |
```

### 3.12 Storage

```markdown
## 12. Storage growth and amplification

| Metric | Initial | Terminal/peak | Delta | Status |
|---|---:|---:|---:|---|
| SQLite database B | `{{B}}` | `{{B}}` | `{{B}}` | report |
| Logical Engine B | `{{B}}` | `{{B}}` | `{{B}}` | report |
| Canonical object B written | 0 | `{{B}}` | `{{B}}` | report |
| Physical DB/canonical amplification | N/A | `{{ratio}}` | N/A | report |
| Maximum transition DB growth B | N/A | `{{B}}` | N/A | report |
| Scratch high-water B | 0 | `{{B}}` | N/A | `{{status}}` |
| Rollback journal peak B | N/A | `{{B|Unavailable}}` | N/A | `{{status}}` |
| Terminal journal/WAL/SHM | absent | `{{value}}` | N/A | `{{status}}` |

| Root range | Transitions | Canonical B written | DB growth B | Amplification |
|---|---:|---:|---:|---:|
| R0→R15 | 15 | `{{B}}` | `{{B}}` | `{{ratio}}` |
| R15→R30 | 15 | `{{B}}` | `{{B}}` | `{{ratio}}` |
| R30→R34 | 4 | `{{B}}` | `{{B}}` | `{{ratio}}` |
```

### 3.13 Resources

```markdown
## 13. Resource closure

| Resource | Hard gate | Observed | Status |
|---|---:|---:|---|
| RSS peak B | `<=33,554,432` | `{{B}}` | `{{status}}` |
| Largest buffer B | `<=1,048,576` | `{{B}}` | `{{status}}` |
| Q high-water B | `<=8,388,608` | `{{B}}` | `{{status}}` |
| Q terminal after every operation B | `0` | `{{max}}` | `{{status}}` |
| Store cache pages | `1,280` | `{{n}}` | `{{status}}` |
| Store spill pages | `1,280` | `{{n}}` | `{{status}}` |
| Store connection high-water | `<=2` | `{{n}}` | `{{status}}` |
| Store connections terminal | `0` | `{{n}}` | `{{status}}` |
| FD baseline/terminal | equal | `{{n}}/{{n}}` | `{{status}}` |
| Product child-process peak | `0` | `{{n}}` | `{{status}}` |
| Terminal child processes | `0` | `{{n}}` | `{{status}}` |
| Owned temp residue | `0` | `{{n}}` | `{{status}}` |
| Journal/WAL/SHM residue | `0` | `{{n}}` | `{{status}}` |
| Live rematerializations | `0` | `{{n}}` | `{{status}}` |
```

### 3.14 Timers

```markdown
## 14. Timer closure

| Row group | Rows | Maximum residual ns | Sum residual ns | Status |
|---|---:|---:|---:|---|
| C03 physical/checkpoint | 15 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| C04 native-history | 3 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| C05 logical/refresh | 15 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| C06 logical-history | 3 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| C07 bursts | 4 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| C08 materialization | 3 | `{{ns}}` | `{{ns}}` | `{{status}}` |
| Complete workflow | 1 | `{{ns}}` | `{{ns}}` | `{{status}}` |

Complete wall: `{{ns}} ns / {{ms}} ms`
Preferred planning range: `<40–45 s`
Hard gate: `<60 s`
```

### 3.15 Failures and unavailable fields

```markdown
## 15. Preserved failures and unavailable observations

| Sequence | Artifact/row | Field | Availability/failure | Reason | Disposition impact |
|---:|---|---|---|---|---|
| `{{n}}` | `{{path/id}}` | `{{field}}` | `{{Unavailable|failure}}` | `{{exact reason}}` | `{{impact}}` |

Preserved failed attempts: `{{count}}`
Superseded attempts: `{{count}}`
Deleted or overwritten attempts: `0`
```

An empty population uses one row containing literal `None`; the heading and
table remain present.

### 3.16 Final disposition

```markdown
## 16. Final disposition

Post-PASS optimization baseline: `{{absolute attempt-007 path}}` (rows SHA-256 `{{digest}}`).

| Optimization metric | Attempt-007 before ms | Current after ms | Absolute gain ms | Owner |
|---|---:|---:|---:|---|
| Complete campaign wall | `{{v}}` | `{{v}}` | `{{v}}` | product + evaluator |
| Transition counter/resource snapshots | `{{v}}` | `{{v}}` | `{{v}}` | evaluator |
| History read/oracle wall | `{{v}}` | `{{v}}` | `{{v}}` | evaluator |
| Append/truncate refresh p50 | `{{v}}` | `{{v}}` | `{{v}}` | product EOF splice |
| Milestone materialization p50 | `{{v}}` | `{{v}}` | `{{v}}` | product read/materialize |
| Verified open `{{root}}` | `{{v}}` | `{{v}}` | `{{v}}` | product retained-union scrub; current scrub/graphs/fetched/object B/scratch=`{{v/v/v/v/v}}` |

Result: `{{PASS|REVISE|FAIL}}`

| Category | Result | Decisive evidence |
|---|---|---|
| Correctness | `{{status}}` | `{{physical/canonical/history counts}}` |
| Durability | `{{status}}` | `{{transactions/COMMITs/reconciliation}}` |
| Locality | `{{status}}` | `{{CDC/unaffected suffix/node bounds}}` |
| Physical routes | `{{status}}` | `{{patch/fallback counts}}` |
| Resources | `{{status}}` | `{{RSS/Q/FD/connections/residue}}` |
| Custody | `{{status}}` | `{{source/executable/fixture/row hashes}}` |
| Complete wall | `{{status}}` | `{{wall}} < 60 s` |

Reason: `{{one concise evidence-backed disposition sentence}}`
```

## 4. `rows.jsonl` exact common schema

`rows.jsonl` contains exactly 47 newline-terminated JSON objects for a complete
run. Row order equals `schedule.json`; no later sorting is allowed.

A valid row is one JSON object that parses, matches the required schema and
scheduled row identity, and contains every required applicable field. Validity
does not imply that the row passed: a schema-valid failed row remains valid and
retained with `status=FAIL` and a non-null `error`.

Required common object shape:

```json
{
  "schema": "layerfs-stage1.1-row-v1",
  "row_index": 0,
  "row_id": "C00-001",
  "row_group": "C00",
  "sequence": 0,
  "epoch": 0,
  "direction": "witness",
  "operation": "admission",
  "size_band": "NotApplicable",
  "status": "PASS",
  "before_bytes": 25165824,
  "after_bytes": 25165824,
  "edit": null,
  "sub_edits": [],
  "history_probes": [],
  "pre_ref": null,
  "post_ref": null,
  "native_route": "NotApplicable",
  "tree_level_before": null,
  "phases": [],
  "phase_counters": [],
  "row_wall_ns": 0,
  "row_residual_ns": 0,
  "counters": {},
  "native": {},
  "storage": {},
  "resources": {},
  "oracle": {},
  "unavailable": [],
  "error": null
}
```

Required enums:

```text
row_group:
  C00 C01 C02 C03 C04 C05 C06 C07 C08 C09

direction:
  witness
  physical-to-logical
  logical-to-physical
  burst

operation:
  admission reset materialize overwrite insert delete append truncate
  verified-history burst milestone-materialize terminal-resources

native_route:
  NotApplicable ExactNoop ClonePatch CloneShift InPlacePatch InPlaceShift FullFallback

status:
  PASS REVISE FAIL
```

`edit`, when present:

```json
{
  "tag": "L22",
  "offset": 16550428,
  "delete_bytes": 16385,
  "insert_bytes": 0,
  "replacement_digest": "<64 lowercase hex>"
}
```

Each C07 `sub_edits` element uses the same fields plus:

```json
{
  "before_bytes": 25165824,
  "after_bytes": 25165824,
  "native_wall_ns": 0,
  "physical_oracle_wall_ns": 0,
  "native_route": "InPlacePatch"
}
```

Each C04/C06 `history_probes` element is retained in exact execution order and
uses:

```json
{
  "root": "R0",
  "ordinal": 1,
  "start": 0,
  "length": 65536,
  "wall_ns": 0,
  "namespace_nodes_read": 1,
  "inode_table_nodes_read": 1,
  "rope_nodes_read": 0,
  "payload_bytes_read": 65536,
  "payload_batch_queries": 0,
  "payload_batch_references": 0,
  "non_payload_statements": 0,
  "non_payload_rows": 0,
  "fetched_rows": 0,
  "authentication_passes": 0,
  "role_decode_passes": 0,
  "engine_counters": {}
}
```

There are exactly 63 such receipts. Within each root, ordinal 1 is the cold
root/path plan lookup and ordinals 2 and 3 must show zero namespace and inode
table reads. Their Engine and operation counters sum exactly to the retained
`history_read` phase and row aggregate.

`pre_ref` and `post_ref`, when present:

```json
{
  "name": "main",
  "generation": 1,
  "root": "<64 lowercase hex>"
}
```

`phases` is an ordered array. Only applicable phases appear:

```json
[
  {"name": "direct_logical_edit", "wall_ns": 0},
  {"name": "changed_root_refresh", "wall_ns": 0},
  {"name": "live_physical_oracle", "wall_ns": 0},
  {"name": "canonical_witness", "wall_ns": 0},
  {"name": "counter_snapshot", "wall_ns": 0}
]
```

Allowed phase names:

```text
admission reset store_open cold_materialization native_edit
direct_logical_edit changed_root_refresh live_physical_oracle
durable_checkpoint verified_open history_read canonical_witness
counter_snapshot milestone_materialization metadata_oracle
explicit_cleanup artifact_write
```

`phase_counters` is the zero-SQL cumulative-counter delta at each applicable
product boundary. The ordered names are:

```text
store_open materialization checkpoint logical_edit apfs_refresh
canonical_witness verified_open history_read
```

Every phase object contains all Engine counter fields plus
`q_before_bytes`, `q_after_bytes`, `q_high_water_bytes`, and
`active_connections`, plus exact VFS-operation
`operation_scratch_tables`, `operation_scratch_statements`,
`operation_scratch_rows`, and `operation_scratch_high_water_bytes`. Additive
Engine phase fields sum exactly; the row's combined scratch fields equal the
documented Engine/VFS aggregate. Session high-water fields use the exact
maximum. Every phase independently
closes fetched/authentication/role, new/incumbent, transaction, publication,
Q, and storage-byte equations.

For a retained-union scrub, report unique-object authentication separately
from per-root graph validation. A scrub-scoped, disk-bounded payload-length
summary is permitted only after the union closure has fetched,
identity-authenticated, and role-decoded that payload. It may replace repeated
payload-byte fetches during per-root extent slice-bound checks, but it must not
skip file-state/mapping validation, namespace reachability, reference counts,
or cleanup, and it must not survive the scrub.

Required `counters` keys:

```text
transactions_started transactions_committed transactions_rolled_back
statements busy_events locked_events
objects_validated objects_created objects_reused
object_bytes_read object_bytes_written
fetched_rows fetched_row_authentication_passes fetched_row_role_decode_passes
new_object_authentication_passes incumbent_authentication_passes
payload_batch_queries payload_batch_references payload_batch_maximum
put_lookup_statements put_insert_statements created_rows reused_rows
publication_commits publication_closure_passes
namespace_graph_verification_passes
scratch_tables scratch_statements scratch_rows scratch_high_water_bytes
cdc_bytes_scanned payload_bytes_written
unaffected_payload_reads unaffected_payload_writes
rope_nodes_read rope_nodes_emitted content_directory_nodes_emitted
workspace_materializations workspace_reuses rematerializations descriptor_resets
```

Required `native` keys:

```text
bytes_read bytes_written patch_bytes suffix_bytes_shifted
clone_attempts clone_successes clone_fallbacks
full_fallback_files files_created files_replaced files_removed
sync_regular_calls sync_directory_calls
```

Required `storage` keys:

```text
database_bytes logical_engine_bytes rollback_journal_bytes temporary_file_bytes
database_growth_bytes canonical_object_bytes_written
physical_to_canonical_amplification
```

Required `resources` keys:

```text
rss_current_bytes rss_peak_bytes
operation_q_current_bytes operation_q_high_water_bytes operation_q_terminal_bytes
fd_current active_store_connections child_processes
owned_temp_entries residue_entries
largest_buffer_bytes page_size cache_pages cache_spill_pages
```

Ordinary row observers use `/dev/fd`, `getrusage`, exact SDK/Engine active
connections, and in-process residue traversal. Therefore
`rss_current_bytes` is `null` with an `Unavailable` record on those rows;
current RSS is sampled only by the decisive external high-water and terminal
observers. `rss_peak_bytes` remains observed on every row.

Required `oracle` keys:

```text
logical_length content_digest physical_bytes_exact canonical_bytes_exact
metadata_exact historical_roots_exact route_exact
```

Unavailable handling:

```json
{
  "storage": {
    "rollback_journal_bytes": null
  },
  "unavailable": [
    {
      "field": "storage.rollback_journal_bytes",
      "availability": "Unavailable",
      "reason": "not continuously observed"
    }
  ]
}
```

`error`, when non-null:

```json
{
  "class": "<typed error class>",
  "message": "<exact stable message>",
  "phase": "<phase name>",
  "first_failed_equation": "<exact equation>",
  "stderr_sha256": "<digest or null>"
}
```

## 5. Row IDs and exact row counts

```text
C00-001                                      1
C01-001                                      1
C02-001                                      1
C03-001 .. C03-015                         15
C04-001 .. C04-003                          3
C05-001 .. C05-015                         15
C06-001 .. C06-003                          3
C07-001 .. C07-004                          4
C08-001 .. C08-003                          3
C09-001                                      1
total                                       47
```

Expected direction by group:

```text
C00/C01/C02/C04/C06/C08/C09  witness
C03                           physical-to-logical
C05                           logical-to-physical
C07                           burst
```

## 6. Statistics object

Every summary population uses this exact machine shape:

```json
{
  "n": 3,
  "raw_ns": [0, 0, 0],
  "sorted_ns": [0, 0, 0],
  "minimum_ns": 0,
  "p50_ns": 0,
  "p95_ns": 0,
  "maximum_ns": 0,
  "range_ns": 0,
  "sum_ns": 0
}
```

Nearest-rank positions:

```text
n=3   p50=x2   p95=x3
n=4   p50=x2   p95=x4
n=5   p50=x3   p95=x5
n=6   p50=x3   p95=x6
n=12  p50=x6   p95=x12
n=15  p50=x8   p95=x15
n=19  p50=x10  p95=x19
n=51  p50=x26  p95=x49
```

`raw_ns` retains schedule order. `sorted_ns` is ascending. Both are required.

## 7. `summary.json` required top-level shape

```json
{
  "schema": "layerfs-stage1.1-summary-v1",
  "status": "PASS",
  "source": {},
  "fixture": {},
  "population": {},
  "roots": {},
  "walls_ns": {},
  "physical_to_logical": {},
  "logical_to_physical": {},
  "refresh_routes": {},
  "bursts": {},
  "history": {},
  "materialization": {},
  "canonical_locality": {},
  "transactions": {},
  "authentication": {},
  "storage": {},
  "resources": {},
  "timer_closure": {},
  "correctness": {},
  "optimization": {},
  "unavailable": [],
  "failures": [],
  "artifacts": {},
  "disposition_reason": ""
}
```

Required `population` values:

```json
{
  "expected_rows": 47,
  "valid_rows": 47,
  "expected_edit_suboperations": 51,
  "observed_edit_suboperations": 51,
  "expected_transitions": 34,
  "observed_transitions": 34,
  "measured_workflows": 1
}
```

Every other object uses exactly these required keys. Aggregate latency values
use the statistics object from section 6; maps named `by_*` use the frozen
operation, size-band, route, root, or row-group labels from `schedule.json`.

```text
source:
  git_commit dirty_tree tree_blake3 manifest_sha256
  release_executable_path release_executable_sha256 release_executable_blake3

fixture:
  master_path master_sha256 fixture_blake3 apfs_identity
  initial_bytes maximum_bytes terminal_bytes master_unchanged

physical_to_logical:
  by_kind by_size_band native_edit durable_checkpoint edit_plus_checkpoint
  count_change_amplification physical_oracle

logical_to_physical:
  by_kind by_size_band direct_logical_edit changed_root_refresh
  logical_edit_plus_refresh physical_oracle

refresh_routes:
  clone_patch in_place_patch patch_aggregate clone_shift in_place_shift
  shift_aggregate insert_shift delete_shift append_shift truncate_shift
  full_fallback_count

bursts:
  by_root aggregate suboperation_count checkpoint_count transaction_count

history:
  sessions aggregate selected_roots verified_open_count probe_count
  first_probe second_probe third_probe first_probe_non_payload_rows
  warm_probe_non_payload_rows

materialization:
  initial by_root milestone_aggregate live_workspace_materializations
  witness_materializations workspace_reuses rematerializations

canonical_locality:
  physical_checkpoints direct_logical_edits save_bursts total
  cdc_bytes_expected cdc_bytes_observed payload_bytes_written
  unaffected_payload_reads unaffected_payload_writes
  maximum_rope_nodes_read maximum_rope_nodes_emitted
  content_directory_nodes_emitted payload_batch_maximum

transactions:
  expected observed committed rolled_back publication_commits
  generation_increment_failures

authentication:
  fetched_authentication_failures fetched_role_decode_failures
  new_object_equation_failures incumbent_equation_failures
  payload_batch_maximum phase_attribution

optimization:
  baseline_run baseline_rows_sha256 baseline_summary_sha256
  complete_wall counter_snapshot_wall history_read_wall verified_open_by_root
  append_truncate_refresh milestone_materialization shift_routes

storage:
  initial_database_bytes terminal_database_bytes initial_logical_engine_bytes
  terminal_logical_engine_bytes canonical_object_bytes_written
  database_growth_bytes maximum_transition_database_growth_bytes
  physical_to_canonical_amplification scratch_high_water_bytes
  rollback_journal_bytes terminal_sidecars by_root_range

resources:
  rss_peak_bytes largest_buffer_bytes operation_q_high_water_bytes
  operation_q_maximum_terminal_bytes page_size cache_pages cache_spill_pages
  store_connection_high_water store_connections_terminal
  fd_baseline fd_terminal product_child_process_peak child_processes_terminal
  owned_temp_residue_entries sidecar_residue_entries
  live_rematerializations network_operations

timer_closure:
  by_row_group maximum_row_residual_ns row_residual_sum_ns
  complete_wall_ns row_wall_sum_ns outside_rows_wall_ns timer_residual_ns
  hard_limit_ns
```

Required `roots` keys:

```text
R0 R5 R10 R15 R20 R25 R30 R31 R32 R33 R34
```

Required `walls_ns` keys:

```text
complete_wall row_wall_sum outside_rows_wall timer_residual
admission reset store_open initial_materialization
physical_phase physical_history_phase logical_refresh_phase
logical_history_phase burst_phase milestone_materialization_phase
cleanup artifact_write
```

Required `correctness` keys:

```text
physical_oracles_expected=51
physical_oracles_passed
canonical_transitions_expected=34
canonical_transitions_passed
save_bursts_expected=4
save_bursts_passed
selected_history_roots_expected=8
selected_history_roots_passed
route_labels_exact
terminal_length_exact
fixture_unchanged
```

Required `artifacts` keys:

```text
environment_sha256 master_sha256 readiness_sha256 schedule_sha256
rows_sha256 rows_line_count campaign_time_sha256
release_executable_sha256 release_executable_blake3
source_tree_blake3 source_manifest_sha256
```

## 8. `campaign-time.txt` exact format

```text
schema=layerfs-stage1.1-campaign-time-v1
status={{PASS|REVISE|FAIL}}
started_unix_ns={{integer}}
completed_unix_ns={{integer}}
complete_wall_ns={{integer}}
row_wall_sum_ns={{integer}}
outside_rows_wall_ns={{integer}}
timer_residual_ns={{integer}}
hard_limit_ns=60000000000
rows_expected=47
rows_valid={{integer}}
edit_suboperations_expected=51
edit_suboperations_observed={{integer}}
transitions_expected=34
transitions_observed={{integer}}
```

The file ends with exactly one newline.

## 9. Disposition algorithm

```text
if any byte/root/history/durability/authentication mismatch: FAIL
else if any row/population/custody/timer equation fails: FAIL
else if any route is mislabeled: FAIL
else if any resource hard gate fails: FAIL
else if complete_wall_ns >= 60,000,000,000: FAIL
else if fixture/master changed or cleanup residue exists: FAIL
else if report-only timing exposes a grounded product bottleneck: REVISE
else: PASS
```

`REVISE` is not permitted for a correctness, resource, custody, timer or
hard-wall failure.

## 10. Final handoff response template

The implementation agent's final user-facing response must follow this exact
order and remain concise. It links artifacts instead of pasting raw arrays.

```markdown
Stage 1.1 completed with disposition **{{PASS|REVISE|FAIL}}**.

| Result | Observed |
|---|---:|
| Valid rows | `{{n}} / 47` |
| Edit/sub-edit operations | `{{n}} / 51` |
| Durable transitions | `{{n}} / 34` |
| Complete workflow | `{{seconds}} s` |
| Physical oracles | `{{n}} / 51` |
| Canonical transitions | `{{n}} / 34` |
| Live rematerializations | `{{n}}` |
| RSS peak | `{{MiB}} MiB` |
| Q high-water / terminal | `{{MiB}} MiB / {{bytes}} B` |
| FD baseline / terminal | `{{n}} / {{n}}` |
| Terminal residue | `{{n}}` |

Logical-to-physical refresh:

| Route | n | p50 | p95 |
|---|---:|---:|---:|
| Patch aggregate | `3` | `{{ms}} ms` | `{{ms}} ms` |
| Insert Shift | `3` | `{{ms}} ms` | `{{ms}} ms` |
| Delete Shift | `3` | `{{ms}} ms` | `{{ms}} ms` |
| Append Shift | `3` | `{{ms}} ms` | `{{ms}} ms` |
| Truncate Shift | `3` | `{{ms}} ms` | `{{ms}} ms` |
| FullFallback | `0` | `N/A` | `N/A` |

Post-PASS optimization versus immutable attempt-007:

| Metric | Before | After | Absolute gain |
|---|---:|---:|---:|
| Complete campaign | `{{ms}} ms` | `{{ms}} ms` | `{{ms}}` |
| Verified open R34 | `{{ms}} ms` | `{{ms}} ms` | `{{ms}}` |
| Append/truncate refresh p50 | `{{ms}} ms` | `{{ms}} ms` | `{{ms}}` |
| Milestone materialization p50 | `{{ms}} ms` | `{{ms}} ms` | `{{ms}}` |
| Evaluator counter/resource snapshots | `{{ms}} ms` | `{{ms}} ms` | `{{ms}}` |

Product gain and evaluator-only gain are labeled separately. The retained
100 MiB focused attribution is reported separately from the 24 MiB campaign.

Decisive disposition: {{one sentence}}.

Artifacts:
- [summary.md]({{absolute path}})
- [summary.json]({{absolute path}})
- [rows.jsonl]({{absolute path}})
- [readiness.json]({{absolute path}})
- [campaign-time.txt]({{absolute path}})

Source: `{{commit}}`; executable SHA-256: `{{digest}}`.

Attempt-007 remains preserved as the accepted pre-optimization baseline.
Stage 1.2 and mounted Stage Two were not started.
```

The final response must not:

```text
call a FullFallback incremental byte projection
call REVISE a PASS
hide failed or unavailable observations
quote only p50 while omitting the retained p95/max/raw artifact
claim current-source performance from an older executable
claim Stage 1.2 or mounted Stage Two eligibility after FAIL
```

## 11. Template verification before implementation

The Stage 1.1 evaluator must have focused tests proving:

```text
47 scheduled rows map one-to-one to the row IDs in section 5
51 edit/sub-edit operations serialize without loss or reorder
34 transitions serialize with exact pre/post RefState
every required common JSON key is present
every unavailable numeric value is null plus an unavailable entry
statistics retain raw and sorted arrays and use exact nearest-rank positions
summary.md section headings and order exactly match section 2
summary tables derive from rows.jsonl, never an independent counter path
campaign-time.txt parses and closes its timer equation
PASS/REVISE/FAIL algorithm rejects every hard-gate mutation
```

No measured row may begin until these template tests and zero-row readiness
pass against the exact release executable.
