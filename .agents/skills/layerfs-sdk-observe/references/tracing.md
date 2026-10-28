# Call trace

> **Status:** Current general guide. Part of the `layerfs-sdk-observe` skill. Describes the
> #333 trace, merged to `main` in `62b225766`. Its source of truth is the call trace
> section of [`api/docs/backend.md`](../../../../api/docs/backend.md#the-call-trace).

## Turning it on

| Setting | Effect |
| --- | --- |
| `LAYERFS_TRACE=<file>` | Append one JSON line per agent and backend call; read when the client is made (`LayerFS.open` takes no options) |
| `LAYERFS_TRACE_STEP=<label>` | Put `step` on each line; read at every call; cut to 200 characters |

Off (unset) means no file, no daemon observation, no recording cost and byte-identical
replies. A failed trace write is dropped and never changes a result. A step label alone
traces nothing.

## One line

```json
{
  "layerfs_trace": 1,
  "id": "c5c6a04c-4",
  "step": "005 A: workspace group",
  "method": "workspace.commit",
  "args": {
    "sandbox": "1",
    "path": "/workspaces/1",
    "message": "save hello.txt",
    "force": false
  },
  "start": "2026-10-15T17:28:16.091Z",
  "scope": "b6941b1af132…",
  "ms": 9.265,
  "ok": true,
  "kind": "Committed",
  "commit": "12c20c75063c…",
  "sizes": {
    "logical": 124,
    "newCanonical": 1736,
    "newStored": 1508,
    "newMetadata": 1073
  },
  "daemon": [
    {
      "timing": []
    },
    {
      "timing": [
        {
          "name": "commit",
          "ms": 3.881,
          "children": [
            {
              "name": "capture",
              "ms": 0.008,
              "children": []
            },
            {
              "name": "producer",
              "ms": 0.063,
              "children": []
            },
            {
              "name": "begin",
              "ms": 1.392,
              "children": []
            },
            {
              "name": "construct",
              "ms": 1.223,
              "children": [
                {
                  "name": "captured.namespace",
                  "ms": 1.205,
                  "children": [
                    {
                      "name": "construct",
                      "ms": 1.203,
                      "children": [
                        {
                          "name": "file",
                          "ms": 0.09,
                          "children": [
                            {
                              "name": "content.runs.small",
                              "ms": 0.002,
                              "children": [
                                {
                                  "name": "content.encode",
                                  "ms": 0,
                                  "children": []
                                },
                                {
                                  "name": "content.identify",
                                  "ms": 0,
                                  "children": []
                                },
                                {
                                  "name": "content.emit",
                                  "ms": 0,
                                  "children": []
                                }
                              ]
                            }
                          ]
                        },
                        {
                          "name": "update",
                          "ms": 0.392,
                          "children": []
                        }
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "name": "head",
              "ms": 0.057,
              "children": []
            },
            {
              "name": "logical.known",
              "ms": 0.007,
              "children": []
            },
            {
              "name": "finish",
              "ms": 0.439,
              "children": []
            },
            {
              "name": "logical.count",
              "ms": 0.132,
              "children": []
            },
            {
              "name": "validate",
              "ms": 0.282,
              "children": []
            },
            {
              "name": "publish",
              "ms": 0.16,
              "children": []
            },
            {
              "name": "install",
              "ms": 0.098,
              "children": []
            }
          ]
        }
      ],
      "storage": {
        "policy": 1,
        "locate": 9,
        "read_packs": 0,
        "read_pack_selections": 8,
        "whole_selected": 8,
        "whole_due_density": 0,
        "whole_due_singleton": 0,
        "whole_due_small": 8,
        "whole_due_reuse": 0,
        "range_selected": 0,
        "range_scan_bytes": 0,
        "range_acquired_bytes": 0,
        "range_materialized_bytes": 0,
        "value_groups": 3,
        "signatures": 1,
        "reserve": 1,
        "initial_reservations": 1,
        "reservation_refills": 0,
        "publish": 2,
        "payload_reads": 2,
        "payload_read_bytes": 545,
        "pack_read_bytes": 5791,
        "pack_write_bytes": 1508,
        "pooled_packs": 0,
        "pooled_groups": 2,
        "reserved_directory_bytes": 0,
        "ordinal_reservations": 1,
        "locator_hits": 82,
        "locator_misses": 26,
        "locator_evictions": 0,
        "locator_eviction_probes": 0,
        "locator_second_chances": 0,
        "locator_bookkeeping_live_peak_bytes": 152,
        "pack_hits": 89,
        "pack_misses": 8,
        "pack_evictions": 0,
        "pack_evicted_bytes": 0,
        "directory_validations": 189,
        "directory_unshared_walks": 189,
        "directory_entry_bounds": 189,
        "prefetch_group_decodes": 3,
        "forced_seals": 0
      },
      "construction": {
        "inode_rows": 2,
        "entry_rows": 1,
        "inode_pages": 1,
        "entry_pages": 1,
        "change_pages": 6,
        "largest_page": 2,
        "parent_points": 1,
        "base_lookups": 1,
        "headers_written": 1,
        "headers_dropped": 0,
        "values_written": 2,
        "fresh_serials": 1,
        "tombstones_skipped": 0,
        "files_constructed": 1,
        "files_unchanged": 0,
        "symlinks": 0,
        "metadata_built": 1,
        "metadata_patched": 1,
        "record_jobs": 43,
        "header_opens": 8,
        "header_rows": 8,
        "header_points": 11,
        "change_opens": 6,
        "change_rows": 6,
        "change_points": 2,
        "value_opens": 4,
        "value_rows": 8,
        "value_points": 3,
        "fresh_opens": 4,
        "fresh_rows": 4,
        "fresh_points": 16
      }
    }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `layerfs_trace` | Format version, `1` |
| `id` | `<run>-<n>`: per-process run prefix and sequence number. Join by `id`; lines are in completion order |
| `step` | Your step label, when set |
| `method` | Binding method, for example `fs.write`, `workspace.commit`, `backend.store.project.get_storage`. Lines written before #428 use the older `operator.*` names; `RENAMED_METHODS` in `@layerfs/agent` maps them |
| `project` | The project the call names, when it names one |
| `args` | Summary: paths, refs, names, Sandbox number, command, message (cut to 200 characters); contents only as byte counts (`contentBytes`, `oldTextBytes`, `changes`/`changeBytes`) |
| `start`, `ms` | Wall start and elapsed milliseconds in the calling process, including the native queue and daemon round trip |
| `ok` | `true`, or `false` with `code` and `message` |
| `kind` | The result's `kind` |
| `commit`, `layer` | On every successful publishing call: the new Commit, or the head when `UpToDate`; `branch.publish` adds the new `layer` |
| `sizes` | `logical`, `newCanonical`, `newStored`, `newMetadata`, when the reply carries the Commit record (`fs` writes and `workspace.commit` that commit) |
| `scope` | 64-hex daemon observation scope; joins to the daemon records |
| `daemon` | One entry per control exchange (a mount makes at least three); absent if no daemon was reached |

Each `daemon` entry:

| Field | Meaning |
| --- | --- |
| `timing` | Phase trees `{ name, ms, children }` (`outcome`, `incomplete` only when they apply), at most 256 nodes; `omitted` counts the rest |
| `sql` | Every exchange: Store SQL work as `writer` and `readers` deltas of 27 `SqlWork` counters (`statements`, `vm_steps`, `fullscan_steps`, `sorts`, `autoindex_rows`, …); includes concurrent calls' work on the same sessions |
| `delta` | On calls that finished a Save: delta selection work after minus before; `payload` (whole-file and chunk counts, `depth_capped`, `work_exceeded`, `whole_file_depths`, `chunk_depths`) and `metadata` (pooled leaves, `depths`); slot d of a histogram = objects stored at chain depth d |
| `fuse` | On `backend.daemon.workspace.status` / `workspace.unmount` of an attached Workspace: its kernel opcode counts since attach (`lookup`, `getattr`, `read`, `write`, …, `served`, `largest_read`), plus `lookup_missing` (ENOENT lookups) and `served_ns` (request-service wall time) |
| `storage` | On a committing exchange: 42 Save storage counters (`pack_write_bytes`, `locator_hits`, `pack_misses`, …) |
| `construction` | On a committing exchange: 31 captured-namespace construction counters (`files_constructed`, `inode_rows`, …) |
| `diagnosticError` | `{ kind, message }` if the daemon couldn't write its records or the telemetry didn't fit (`Oversized`); the call is unaffected |

## Timing roots

| Root | Where | Children |
| --- | --- | --- |
| `commit` | `workspace.commit` | `capture`, `producer`, `begin`, `construct` › `captured.namespace`, `head`, `merge`, `logical.known`, `finish`, `logical.count`, `validate`, `publish`, `install` |
| `files.bind`, `commit` | `fs` writes (private Workspace) | as above, without storage/construction counters |
| `sandbox.mount.bind` › `store.bind` | `workspace.mount` | |
| `sandbox.mount.attach` › `kernel.attach` | `workspace.mount` | |
| `sdk.read`, `sdk.lines` | `fs.read`, line reads | `content.read` |
| `branch.merge` | `branch.merge` | merge tree |
| `branch.merge.publish` | `branch.merge`, a clean merge | the one history write that publishes it: the merge's writer hold |
| `blame.compare` | `fs.blame` | |

## Reading it

```bash
# slowest calls
jq -c 'select(.ms) | [.ms, .method, .step] ' trace.jsonl | sort -rn | head
# per-method p50 / max (needs jq 1.6+)
jq -s 'group_by(.method) | map({method: .[0].method, n: length, p50: (map(.ms) | sort | .[length/2|floor]), max: (map(.ms) | max)})' trace.jsonl
# phase breakdown of one slow Commit
jq -c 'select(.id == "9f3c21aa-12") | .daemon[].timing' trace.jsonl
# refusals
jq -c 'select(.ok == false) | [.id, .step, .method, .code, .message]' trace.jsonl
```

Overhead with tracing on, from #333's one-sample diagnostic: about 1–2 ms per
single-exchange call and 3–6 ms per multi-exchange call (mount, status, commit,
unmount). Compare times only within one tracing setting.
