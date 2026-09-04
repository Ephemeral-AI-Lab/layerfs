# Payload creation and random reads

> **Status:** Current v0.1.3 planning specification; no release candidate or
> measured result is implied.

Family ID: `payload_create_read`. This v0.1.3 implementation contract contains
**8 new timed cases, 2 inherited timed anchors, and no standalone proofs**.
New cases are specified here; their runner adapters are not implemented yet.

## Purpose and boundary

Measure ordinary filesystem payload creation and bounded random reads through
real FUSE. These cases complement the inherited singular SDK edits. Whole-tree
reads belong to [directory construction and traversal](directory-construction-traversal.md).

Use the [shared testing rules](testing-rules.md) for seeds, sample isolation,
source seals, preparation reuse, resource limits, timing fields, and verification.
Each new sample has one Store, genesis Layer, Branch, Workspace, fresh workload
process, final Commit attempt, and clean End. Use public
`Client::create_workspace_session`, managed Workspace execution,
`Client::commit_workspace_session`, and `Client::end_workspace_session`.

## Cases and fixtures

In the following table, expand `N` over exactly `1, 10, 100, 500`. Braces denote
scenario-ID expansion, not implemented command-line syntax.

| Scenario IDs | Cases | Initial state and measured work | Commit outcome |
| --- | ---: | --- | --- |
| `payload-create-{N}m` | 4 | Empty genesis; create one exact N MiB file through ordinary writes, then `sync_all` | `Created` |
| `payload-random-read-{N}` | 4 | Prepared 500 MiB file; execute the first N deterministic 4 KiB reads | `UpToDate` |
| `cold-create-32m` | 1 inherited | Released empty-Branch 32 MiB creation lifecycle | Frozen outcome |
| `read-32m` | 1 inherited | Released complete 32 MiB sequential-read lifecycle | Frozen outcome |

Reuse exact sizes, payload bytes, SHA-256 values, canonical roots, and bounded
`fixture_block` generation from
[`sdk_edit_common.rs`](../../../../benchmark/fs-bench-pro/families/sdk_edit_common.rs).
The 1/10/100 MiB payloads remain exact prefixes of the existing 500 MiB payload.
Prepared source bytes may be reused; each measured create must write every
result byte into its fresh empty Workspace. Never substitute a prepared
result Store or move creation into setup.

For each shared seed, derive one 500-request schedule using:

```text
h = SHA256(seed_label || 0x00 || "payload-random-read" || 0x00 || index_le_u64)
offset = little_endian_u64(h[0..8]) mod (524288000 - 4096 + 1)
```

Requests use ordinary offset reads and preserve the listed order. The four
read cases use schedule prefixes against the same payload. The largest case
requests 2,048,000 bytes; it is a random-access measurement, not a full-file
throughput claim. The independent transcript digest covers each offset,
returned length, and returned bytes.

A workload contains at most one regular file of 524,288,000 bytes. Neither
curve creates a second payload copy in the tested namespace. Source caches
remain outside that namespace and have the separately reported preparation
budget in the shared rules. The inherited anchors retain their IDs, fixtures,
seeds, sample counts, timing fields, and verification meanings unchanged.

## Timing and evidence

For new cases, measure Workspace Create, inner workload, ordinary sync where
applicable, Commit return, required visibility, End, and complete lifecycle
separately. Payload reads and writes occur inside the workload timer. Fixture
construction, Store preparation, source sealing, and independent verification
have separate walls and never enter operation latency distributions.

Record requested/completed bytes and requests, create throughput, random-read
transcript identity, FUSE calls and transferred bytes, CPU, memory, swap/OOM,
Store/object changes, and cleanup using the shared schema. Report read-only
Store growth separately from writes; reads must not create payload objects or
change the committed root merely because bytes were read.

The separate verifier reconnects to the Store and mounts a fresh FUSE Workspace.
It compares the complete path/type/size/mode/mtime/content manifest with the
independent expected manifest, authenticates content and canonical roots,
checks exact Branch head and Commit outcome, and proves cleanup. Random reads
also match the byte oracle at every scheduled offset. Creation must preserve
its exact payload after reconnect; read-only cases preserve the genesis state.

## Execution and completion

There are three fresh performance samples per new case, one per shared seed:
**24 new timed executions**, with separate verification of every case. Inherited
anchors follow their own frozen sampling and are accounted once.

Reuse the existing benchmark binary, workload helper, runner selection,
custody/sample-clone mechanism, and reports. Extend only their family adapters.
The prospective development selector is one scenario and one seed, followed
by its focused verification; this selector is a requirement, not an available
command. Reuse validated inputs and prepared read Stores, never mutable sample
outputs. Start with `payload-create-1m` or `payload-random-read-1`.

The selected ordinary development target is provisionally 1–5 seconds where
baseline evidence supports it. Larger payloads, complete families, and full
verifiers use the longer lane with baseline-derived budgets. Completion requires
all eight cases, their independent proofs, exact size bounds, resource/cleanup
gates, and retained raw samples. No additional payload-size × request-count
matrix is introduced.
