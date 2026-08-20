# CP-NNNN — OPERATION: one-variable hypothesis

Status: `BASELINE | SCREEN-PASS | RETAIN | REVISE | REVERT | INCONCLUSIVE`
Date: `YYYY-MM-DD`
Experiment mode: `screening | acceptance | baseline`
Primary operation: `durable-full-write | edit-same | edit-plus1 | materialize-warm | materialize-fresh | read-range | reopen`
Total experiment wall: `HH:MM:SS`
Retained artifact bytes: `N`
Transient databases deleted: `yes | no + reason`

## 1. Checkpoint identity

| Field | Value |
|---|---|
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| Parent checkpoint | `CP-NNNN` |
| Control commit | `<full SHA>` |
| Candidate commit | `<full SHA>` or `working-tree` |
| Candidate diff SHA-256 | `<hash>` or `NotApplicable` |
| Control executable SHA-256 | `<hash>` |
| Candidate executable SHA-256 | `<hash>` |
| Fixture manifest SHA-256 | `<hash>` |
| Raw JSONL SHA-256 | `<hash>` |
| Rust / SQLite | `<versions>` |
| Host / OS | `<machine and OS>` |

Dirty evidence is identified by `HEAD + complete diff SHA-256 + executable
SHA-256`, never by `HEAD` alone.

## 2. One changed variable

Changed component: `<one function/path/component>`

Before:

```text
<brief old behavior>
```

Candidate:

```text
<brief new behavior>
```

Explicitly unchanged:

```text
CDC boundaries and sequence
ChunkId/ObjectId and canonical bytes
mapping profile and schema
FULL + DELETE durability
one transaction and one COMMIT
```

## 3. Hypothesis and bounds

Expected mechanism:

```text
<one duplicated pass, copy, crossing, or algorithmic unit removed>
```

Expected direct-counter equation:

```text
<control -> candidate prediction>
```

Minimum useful effect: `<for example, affected median improvement >= 5%>`

| Property | Control | Candidate |
|---|---|---|
| Time bound | `<bound>` | `<bound>` |
| Live-memory bound | `<bound>` | `<bound>` |
| Durable metadata bound | `<bound>` | `<bound>` |

## 4. Test contract

| Fixture | Bytes | Seed | SHA-256 | CDC refs / sequence |
|---|---:|---|---|---|
| S1-1 | 1,048,576 | `<seed>` | `<hash>` | `<count> / <hash>` |
| S1-10 | 10,485,760 | `<seed>` | `<hash>` | `<count> / <hash>` |
| S1-100 | 104,857,600 | `<seed>` | `<hash>` | `<count> / <hash>` |

Operations and samples:

| Operation | 1 MiB | 10 MiB | 100 MiB |
|---|---:|---:|---:|
| Primary affected operation | `<runs>` | `<runs>` | `<schedule>` |
| Protected operation 1 | `<runs>` | `<runs>` | `<runs>` |
| Protected operation 2 | `<runs>` | `<runs>` | `<runs>` |

Schedule:

```text
<warmup and exact A/B order fixed before execution>
```

Timer equation:

```text
<disjoint equation for the primary user-facing operation>
```

`materialize-fresh` means a fresh process and SQLite connection without
LayerFS application cache. Record OS page-cache state as uncontrolled unless
it is directly controlled and evidenced.

## 5. Correctness gate

| Check | Result |
|---|---|
| Source fingerprint and ordered CDC | `PASS / FAIL` |
| Canonical bytes and object IDs | `PASS / FAIL` |
| Mapping/workspace root and transition/delta | `PASS / FAIL` |
| Fresh closure authentication | `PASS / FAIL` |
| Reconstruction and requested ranges | `PASS / FAIL` |
| One transaction and one COMMIT | `PASS / FAIL` |
| Exact typed failure provenance | `PASS / FAIL / NotAffected` |
| Exact Q high-water and terminal zero | `PASS / FAIL` |
| Focused malformed/tamper tests | `PASS / FAIL / NotAffected` |

Any identity, closure, malformed-input, transaction, durability, provenance,
or exact-Q failure forces `REVERT` regardless of wall time.

## 6. Primary performance result

| Metric | Control median | Candidate median | Delta | Paired wins | Gate |
|---|---:|---:|---:|---:|---|
| Primary operation wall | `<ms>` | `<ms>` | `<percent>` | `<N/M>` | `PASS / FAIL` |
| Primary throughput, if valid | `<rate>` | `<rate>` | `<percent>` | `<N/M>` | `PASS / FAIL` |
| Affected child timer | `<ms>` | `<ms>` | `<percent>` | `<N/M>` | `diagnostic` |

Paired deltas:

```text
pair 1: <percent>
pair 2: <percent>
pair 3: <percent>
paired median: <percent>
min / max / spread: <values>
```

Never report whole-source throughput for a small edit.

## 7. Protected operations

| Operation | Control | Candidate | Delta | Gate |
|---|---:|---:|---:|---|
| Durable full write | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |
| Same-count edit | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |
| Warm materialization | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |
| Fresh-process materialization | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |
| Range read | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |
| Reopen | `<value>` | `<value>` | `<percent>` | `PASS / FAIL / NotAffected` |

## 8. Direct counters

| Counter | Control | Candidate | Expected | Result |
|---|---:|---:|---|---|
| Source bytes/read calls | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| CDC refs/boundaries | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| Raw/canonical hashes and bytes | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| Canonical/mapping bytes | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| Objects created/reused/authenticated | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| SQL query/execute calls | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| BLOB reads/writes/bytes | `<value>` | `<value>` | `<prediction>` | `PASS / FAIL` |
| Transactions/COMMITs | `<value>` | `<value>` | `1 / 1` | `PASS / FAIL` |

A wall improvement without the predicted direct-counter movement is
`INCONCLUSIVE`, not causal evidence.

## 9. Resource and storage guard

| Metric | Control | Candidate | Delta | Gate |
|---|---:|---:|---:|---|
| User CPU | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |
| System CPU | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |
| Exact Q high-water / terminal | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |
| Peak RSS / footprint | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |
| Logical/apparent/allocated store | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |
| Metadata / W / D | `<value>` | `<value>` | `<percent>` | `PASS / FAIL` |

Unsupported physical observations remain `Unavailable`; do not substitute
logical, pager, or apparent bytes.

## 10. Decision

Decision: `BASELINE | SCREEN-PASS | RETAIN | REVISE | REVERT | INCONCLUSIVE`

Reason:

```text
<brief evidence-backed disposition>
```

Controlling facts:

```text
performance gate: <PASS / FAIL>
paired wins: <N/M>
predicted counter movement: <PASS / FAIL>
correctness: <PASS / FAIL>
resources: <PASS / FAIL>
```

Next action:

```text
<one concrete next step>
```

Do not stack the next optimization on this checkpoint unless the decision is
`RETAIN`.

## 11. Reproduction and compact evidence

Focused test:

```bash
<exact command>
```

Build:

```bash
<exact command>
```

Benchmark:

```bash
<exact command>
```

Raw evidence:

```text
file: cp-NNNN-<state>-<experiment>.jsonl
SHA-256: <hash>
rows: <count>
bytes: <count>
```

No database, generated fixture, copied authority file, output file, or release
executable is retained in this report directory.
