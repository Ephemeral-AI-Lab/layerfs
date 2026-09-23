# Telemetry ingestion and retention

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is the first-pass benchmark treatment of the existing `layerfs-telemetry`
output. The #235 runner implements `forward` ingestion; admission remains
conditional on the per-run zero-loss and custody checks below.
The [selection and metric rules](parameters-and-telemetry.md) still govern what
a row may claim. This page defines how an agent reads the records and when
temporary telemetry output can be removed.

## Collection boundary

Use `LAYERFS_TELEMETRY=forward` for a telemetry-required performance selection.
Capture Service and daemon stderr separately from product/protocol stdout. A
Service on macOS samples its own process; a daemon on Linux samples its own
process whether it runs inside or outside a sandbox. `storage-direct` uses the
same native runtime in its benchmark process. Record each actual placement and
the common run ID, distinct namespace, role and PID. These process observations
do not measure the whole host or sandbox. Capture host caller and container
cgroup resources in their separately named benchmark scopes.

`LFT1 ` is the literal telemetry line prefix; the following JSON has `"v":1`.
It is a record-format marker, not a benchmark sample, test ID or timing phase.
One `LFT1 ` line contains one JSON event. The native monitor samples internally
every 100 ms but emits periodic `resource` lines about once per second;
`operation` lines carry the bounded timing tree and sampled process window.
Do not expect a raw line for every internal sample. `local` and `both` are
separate output-profile selections, not an additional evidence source for a
`forward` performance row.

## How operation time is recorded

`Timing::record` starts a monotonic `Instant` before the operation body runs.
Each instrumented child starts its own local timer. On return, the crate writes
an inclusive `elapsed_ns` on the root and each completed child, with the
original outcome and an `incomplete` marker when detail was clipped. The
native `LFT1` operation event carries that `timing` tree; it does **not** carry
fields named `M1`, `M2`, `M3` or `M4` or a cross-process start timestamp.

```text
LFT1 { ..., "kind":"operation", "role":1,
       "timing": {"name":"ConstructFile", "elapsed_ns":<duration>,
                  "children":[
                    {"name":"service.begin_save", "elapsed_ns":<duration>, ...},
                    {"name":"service.construct",  "elapsed_ns":<duration>, ...},
                    {"name":"service.finish",     "elapsed_ns":<duration>, ...}
                  ]}, ... }
```

The values above are a shape illustration, not a measured record. The report
groups real labels under the four macro headings; it preserves the original
labels and durations in `telemetry.lft1`:

| Report heading | Actual source and field | Current availability |
| --- | --- | --- |
| M1 public workflow | #230 caller's one `operation_ns` in its case receipt | Proposed runner; not emitted by current daemon/Service telemetry |
| M2 delivery | Daemon and Service `LFT1` operation roots, each `.timing.elapsed_ns` | Two local durations exist; a transport-only duration does not |
| M3 C1/C2 | Named descendants of the Service timing tree | Partial; construction/edit spans include C2 handoff work |
| M4 C5 | Future named Service descendants for reservation, stage and Commit | C5 is inside the Service root today, without its own duration |

One initial #231 case has `sample_count=1` even when it contains many `LFT1` operations
or several Commits. A parent duration includes its children, and clocks in
different processes are independent. Do not add all four headings, subtract
daemon from Service to estimate transport, or report missing spans as zero.

## Parse and check before deriving a result

1. Drain every selected process's stderr without blocking product stdout.
   Preserve each producer's line order and record capture truncation, exit and
   timeout status. The primary Docker route uses separate stdout and stderr
   attachments as specified by the
   [Service runtime](../../architecture/14-service-runtime.md).
2. Copy every complete `LFT1 ` line **byte-for-byte** into one
   `telemetry.lft1` file for the case. Concatenate producers in a recorded order;
   cross-process line order has no timing meaning. Record each producer's byte
   range, role, namespace, PID, line count and SHA-256 in `receipt.json` so its
   original stream can be recovered from that one file. Keep startup and other
   non-`LFT1` stderr diagnostics, including their source, in the receipt as
   bounded failure context; truncation makes required evidence incomplete.
3. Parse the retained lines, not a separately rewritten copy. Require the
   `LFT1 ` prefix, valid JSON, `v=1`, recognized `operation`, `resource`,
   `resource-unavailable` or `run-summary` kind, and the required fields and
   types for that kind. Bind events to the selected run and producer identity.
   A request `key` is local to its session and is not a cross-process or
   globally unique join key.
4. Check each registered operation's expected record count and label, tree
   completeness, result and run-summary presence. Record missing/extra lines,
   source and identity mismatches, capture truncation, and reported
   `dropped`/`failed`/`overflow` separately. A zero-loss run summary alone is
   insufficient: output is asynchronous and best effort, so expected-event
   cardinality and capture integrity must also pass. Never synthesize a
   missing event or discard a failing row.
5. Keep `sampled_max_rss` as sampled **process** RSS and `cpu_shared_ns` as a
   shared-process sample delta. `resource_status=sampled` means only that a
   sample was present; the first sample may predate the operation. Check
   `opened_ns`, `first_ns`, `last_ns`, `closed_ns`, samples, gaps and largest
   gap before claiming coverage. Null or insufficient CPU/RSS stays unavailable,
   not zero. Do not add overlapping timer nodes, combine process clocks, or
   substitute these samples for phase-qualified host/cgroup memory evidence.

Index parsed operation nodes by the
[four macro timing scopes](pipeline-and-modes.md#four-macro-timing-scopes),
retaining each raw label and process/clock domain. A node missing from today's
product is `null` with its missing-span reason. Parsing does not create a
Workspace, transport, pure-C1, or C5 duration that the product did not emit.
Many operation lines within one case remain micro events of its **one**
performance sample.

The case receipt names the retained `telemetry.lft1` file, its manifest hash,
the parser version, per-producer identities and event counts, availability and
loss status, and the exact timing/resource fields used by the report. Derived
tables are reproducible from that file, the other retained raw benchmark inputs
and the sealed report generator. Agents can inspect the retained source with:

```sh
sed -n 's/^LFT1 //p' telemetry.lft1 |
  jq -c 'select(.kind == "operation") |
    {role, namespace, pid, key, name: .timing.name,
     elapsed_ns: .timing.elapsed_ns, success, resource_status,
     samples, gaps, cpu_shared_ns, sampled_max_rss}'
```

This command reads an already retained case file; it is not a benchmark runner
command. Inspect `kind == "run-summary"` in the same file for loss counters.

## Recycle temporary telemetry output

The retained case needs **one raw telemetry file**, not duplicate stderr logs,
native segment copies, a second parsed event dump and per-sample files.
Capture files/pipes and any benchmark-owned `local`/`both` segments are temporary.
After all producers exit and their streams reach EOF, validate the parser and
source ranges, close and hash `telemetry.lft1`, and verify its retained bytes.
Then remove only those temporary telemetry artifacts. Record the removal result
as cleanup, and publish the final receipt and manifest with hashes of every
retained file. Removal happens outside the product timer and its wall cost
belongs to cleanup.

This applies to PASS, FAIL and INELIGIBLE attempts: their **retained** raw
events, diagnostics, statuses and receipts remain append-only. If capture,
parsing, transfer or hash validation is incomplete, retain the original
source stderr in that attempt's unique evidence directory, mark telemetry
`INCOMPLETE`, and do not delete the only copy. An output-profile test may remove
only the operational segment paths it exclusively owns, after it has recorded
the required segment/rotation/permission evidence. Never recycle another
process's logs, prepared fixtures, historical receipts or a comparator arm.

The native output queue's expiry is an operational retention policy, not the
benchmark evidence policy. Do not point `LAYERFS_TELEMETRY_DIRECTORY` at the
append-only benchmark directory. Successful ingestion permits immediate cleanup
of owned temporary telemetry output; it does not permit rewriting or removing
`telemetry.lft1`, `receipt.json` or the evidence manifest.
