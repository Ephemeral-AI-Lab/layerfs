#!/usr/bin/env python3
"""Decode the daemon's numeric observation records into named JSON.

An observed control call (`Request::Observed`) makes the daemon write fixed
schema lines to its stderr, so to `docker logs`:

    {"layerfs_observation":1,"call":N,"slot":N,"scope":[4 words],
     "daemon":[4 words],"section":S,"index":I,"available":B,"values":[...]}

This tool reads such a stream (other lines are skipped), groups the records of
one call by daemon, scope and call, names every section and value from the
source order in engine/crates/layerfs-daemon/src/application/diagnostics/
(diagnostics.rs, schema.rs, kernel.rs, resources.rs), and prints one JSON object
per call. With --trace it adds the SDK trace line id whose `scope` matches, so
each traced SDK call links to its daemon records.

    docker logs CONTAINER 2>&1 | python3 tools/decode_observations.py --trace trace.jsonl

Unavailable sections stay explicit as null; they are never zero. A group with
no closing section 99 is reported with "complete": false. Values are the
daemon's cumulative counters at that call, not per-call deltas.
"""

import argparse
import json
import sys

PREFIX = '{"layerfs_observation":'

OPERATIONS = {
    1: "Mount",
    2: "Commit",
    3: "Status",
    4: "Unmount",
    5: "Fork",
    6: "History",
    7: "Hello",
    8: "EndSession",
    9: "Attach",
    10: "Locate",
    11: "ForceUnmount",
    12: "Observed",
    13: "Cleanup",
    14: "Branches",
    15: "Sdk",
}
STATEMENT_KINDS = [
    "startup",
    "begin",
    "commit",
    "rollback",
    "inode",
    "directory_entry",
    "payload",
    "frontier",
    "capture",
    "operation_record",
    "explain",
    "reclaim",
]
OPCODES = [
    "lookup",
    "forget",
    "batch_forget",
    "getattr",
    "setattr",
    "readlink",
    "mknod",
    "mkdir",
    "unlink",
    "rmdir",
    "symlink",
    "rename",
    "link",
    "open",
    "read",
    "write",
    "flush",
    "release",
    "fsync",
    "opendir",
    "readdir",
    "readdirplus",
    "releasedir",
    "fsyncdir",
    "statfs",
    "setxattr",
    "getxattr",
    "listxattr",
    "removexattr",
    "access",
    "create",
    "getlk",
    "setlk",
    "bmap",
    "ioctl",
    "poll",
    "fallocate",
    "lseek",
    "copy_file_range",
]
SQL_PHASES = ["prepare", "bind", "next", "map", "drop_cursor", "status", "drop_statement"]

STATEMENT = [
    "attempts",
    "executions",
    "rows_returned",
    "rows_changed",
    "direct_rows_changed",
    "returned_blob_bytes",
    "returned_value_bytes",
    "vm_steps",
    "fullscan_steps",
    "sorts",
    "autoindex_rows",
    "reprepares",
    "bound_bytes",
    "sql_bytes",
    "statement_memory_samples",
    "statement_memory_sample_bytes",
    "elapsed_ns",
]
PAYLOAD = [
    "write_input_bytes",
    "write_cells",
    "partial_write_cells",
    "cell_copy_bytes",
    "cell_zeroed_bytes",
    "read_window_zeroed_bytes",
    "read_local_copy_bytes",
]
ALLOCATION = ["attempts", "requested_bytes", "admitted_jobs", "refusals", "observations", "freelist_queries"]
SQL = (
    [
        "statements",
        "vm_steps",
        "fullscan_steps",
        "sorts",
        "autoindex_rows",
        "reprepares",
        "returned_rows",
        "bound_bytes",
        "statement_ns",
        "commit_ns",
        "transactions",
        "write_transactions",
        "write_commits",
        "commits",
        "read_snapshots",
        "rollbacks",
        "transaction_ns",
        "read_snapshot_ns",
        "sealed_inserts",
        "sealed_body_bytes",
        "blob_open_calls",
        "blob_reopen_calls",
        "blob_read_calls",
        "blob_requested_bytes",
        "blob_read_bytes",
        "blob_read_ns",
        "blob_close_calls",
    ]
    + [f"statement_{p}_{f}" for p in SQL_PHASES for f in ("calls", "wall_ns")]
    + [f"commit_{p}_{f}" for p in SQL_PHASES for f in ("calls", "wall_ns")]
)
STORAGE = [
    "policy",
    "locate",
    "read_packs",
    "read_pack_selections",
    "whole_selected",
    "whole_due_density",
    "whole_due_singleton",
    "whole_due_small",
    "whole_due_reuse",
    "range_selected",
    "range_scan_bytes",
    "range_acquired_bytes",
    "range_materialized_bytes",
    "value_groups",
    "signatures",
    "reserve",
    "initial_reservations",
    "reservation_refills",
    "publish",
    "payload_reads",
    "payload_read_bytes",
    "pack_read_bytes",
    "pack_write_bytes",
    "pooled_packs",
    "pooled_groups",
    "reserved_directory_bytes",
    "ordinal_reservations",
    "locator_hits",
    "locator_misses",
    "locator_evictions",
    "locator_eviction_probes",
    "locator_second_chances",
    "locator_bookkeeping_live_peak_bytes",
    "pack_hits",
    "pack_misses",
    "pack_evictions",
    "pack_evicted_bytes",
    "directory_validations",
    "directory_unshared_walks",
    "directory_entry_bounds",
    "prefetch_group_decodes",
    "forced_seals",
]
PAGES = [
    "pages_read",
    "read_waves",
    "pages_created",
    "pages_reused",
    "change_keys",
    "untouched_subtrees",
    "peak_scratch_bytes",
]

# section -> (name, fields, how the index is named)
SECTIONS = {
    0: ("operation", ["operation", "configured_readers", "cache_bytes"], "namespace"),
    1: (
        "owner",
        [
            "jobs",
            "maintenance_jobs",
            "maintenance_rows",
            "closed_namespaces",
            "maintenance_data_bytes",
            "maintenance_ns",
            "jobs_general",
            "jobs_mutation",
            "jobs_source",
            "lock_wait_ns_general",
            "lock_wait_ns_mutation",
            "lock_wait_ns_source",
            "hold_ns_general",
            "hold_ns_mutation",
            "hold_ns_source",
        ],
        None,
    ),
    2: ("sql_foreground", STATEMENT, "statement"),
    3: ("sql_maintenance", STATEMENT, "statement"),
    4: ("payload_foreground", PAYLOAD, None),
    5: ("payload_maintenance", PAYLOAD, None),
    6: ("allocation_foreground", ALLOCATION, None),
    7: ("allocation_maintenance", ALLOCATION, None),
    8: ("store", ["object_batches", "object_ids", "length_batches", "length_ids", "serial_reservations"], None),
    9: (
        "store_readers",
        ["stopping", "poisoned", "readers", "quarantined", "opening_failures", "requests", "lock_wait_ns"],
        None,
    ),
    10: (
        "immutable_cache",
        [
            "cache_hits",
            "cache_misses",
            "upstream_batches",
            "authenticated_bytes",
            "evictions",
            "charged_cache_bytes",
            "cached_objects",
            "seeded_objects",
            "seeded_bytes",
            "seeded_lengths",
        ],
        None,
    ),
    12: ("kernel_mount", ["owner_id", "root_serial", "serving", "detached", "abort_bound"], None),
    13: ("writer_sql", SQL, None),
    14: ("reader_sql", SQL, "reader"),
    15: (
        "commit_namespace",
        [
            "inode_rows",
            "entry_rows",
            "inode_pages",
            "entry_pages",
            "change_pages",
            "largest_page",
            "parent_points",
            "base_lookups",
            "headers_written",
            "headers_dropped",
            "values_written",
            "fresh_serials",
            "tombstones_skipped",
            "files_constructed",
            "files_unchanged",
            "symlinks",
            "metadata_built",
            "metadata_patched",
            "record_jobs",
            "header_opens",
            "header_rows",
            "header_points",
            "change_opens",
            "change_rows",
            "change_points",
            "value_opens",
            "value_rows",
            "value_points",
            "fresh_opens",
            "fresh_rows",
            "fresh_points",
            "window_jobs",
            "normalize_ns",
            "values_ns",
            "update_ns",
            "inode_page_ns",
            "file_ns",
            "file_bind_ns",
            "file_source_ns",
            "values_emit_ns",
            "values_record_ns",
            "update_record_ns",
            "update_capture_ns",
            "update_emit_ns",
            "backing_calls",
            "backing_keys",
            "backing_ns",
        ],
        None,
    ),
    16: ("commit_storage", STORAGE, None),
    17: ("content_objects", ["objects_read", "read_waves", "bytes_read", "objects_emitted", "bytes_emitted"], None),
    18: ("content_directories", PAGES, None),
    19: ("content_inodes", PAGES, None),
    20: (
        "content_references",
        [
            "rows_touched",
            "rows_spilled",
            "base_records_read",
            "base_waves",
            "final_values",
            "final_removals",
            "serials_scanned",
            "peak_pending",
            "runs_rows_written",
            "runs_rows_read",
            "runs_created",
            "runs_merges",
            "runs_peak_level",
            "runs_peak_live_runs",
            "runs_peak_run_bytes",
        ],
        None,
    ),
    21: (
        "content_release",
        ["pages", "entries", "base_records", "released", "traversed_directories", "peak_depth"],
        None,
    ),
    22: (
        "content_validation",
        [
            "objects_read",
            "read_waves",
            "inode_demands",
            "inode_pages_read",
            "directory_pages_read",
            "entries_examined",
            "inode_pages_allocation",
            "inode_pages_prefetch",
            "inode_pages_bindings",
            "inode_pages_aliases",
            "inode_pages_cycles",
            "inode_pages_reachability",
            "placements",
            "ancestry_steps",
            "territory_directories",
            "territory_entries",
            "peak_window_rows",
            "in_place_scans",
            "in_place_rows",
            "in_place_directories",
        ],
        None,
    ),
    23: ("content_bindings", ["bindings_added", "bindings_removed", "directory_updates", "base_records_read"], None),
    24: ("kernel_loops", ["phase", "revision", "configured", "created", "entered", "exited", "joined"], None),
    25: ("kernel_work", ["active", "retained", "requests", "completed", "terminal"], None),
    26: (
        "kernel_opcodes",
        OPCODES
        + ["served", "inline", "refused", "terminal", "forget_units", "store_units", "lookup_missing", "served_ns"],
        None,
    ),
    27: ("kernel_abort", ["disposition", "value"], None),
    28: (
        "workspace_resources",
        [
            "namespaces",
            "inode_rows",
            "directory_entry_rows",
            "payload_cells",
            "payload_bytes",
            "shrink_rows",
            "operation_record_rows",
            "operation_record_bytes",
            "owner_rows",
            "source_rows",
            "owner_details",
            "reply_tickets",
            "maintenance_targets",
            "ready_targets",
            "logical_bytes",
            "allocated_bytes",
            "high_water_allocated_bytes",
            "reserved_tail_bytes",
            "cleanup_headroom_bytes",
        ]
        + [f"allocation_{f}" for f in ALLOCATION]
        + ["database_pages", "free_pages", "debt_upper_bytes"],
        None,
    ),
    29: ("reader_storage", STORAGE, "reader"),
    30: (
        "resource_observer",
        ["attempted", "completed"]
        + [f"sql_{f}" for f in STATEMENT]
        + [f"payload_{f}" for f in PAYLOAD]
        + [f"allocation_{f}" for f in ALLOCATION]
        + ["lock_wait_ns", "hold_ns"],
        None,
    ),
    32: (
        "open_state",
        [
            "kernel_entries",
            "directory_parents",
            "file_handles",
            "file_reads",
            "lookup_owners",
            "file_custody",
            "orphan_files",
            "generation_waits",
            "mounts",
            "directory_handles",
            "cookie_pages",
        ],
        None,
    ),
    33: ("template_memory_management", ["enabled"], None),
    34: ("resource_jobs", ["jobs"], None),
    35: ("owner_lock_holds", ["observations", "reply_attempts", "reply_hold_ns"], None),
    36: ("statement_families", ["count"], None),
    37: ("frontier_classes", ["count"], None),
    38: ("resource_scope", ["workspace"], None),
    39: ("reader_schema", ["version"], None),
    99: ("footer", ["records", "numbers", "unavailable"], None),
}


def words_hex(words):
    return "".join(f"{word:016x}" for word in words)


def named(section, index, available, values):
    name, fields, indexed = SECTIONS.get(section, (f"section_{section}", [], None))
    if indexed == "statement" and 0 <= index < len(STATEMENT_KINDS):
        name = f"{name}.{STATEMENT_KINDS[index]}"
    elif indexed == "reader":
        name = f"{name}.{'closed_sum' if index == -2 else index}"
    if not available:
        return name, None
    value = {field: values[i] for i, field in enumerate(fields) if i < len(values)}
    extra = values[len(fields) :]
    if extra:
        value["unnamed"] = extra
    if section == 0:
        value["operation"] = OPERATIONS.get(values[0], values[0])
        value["namespace"] = index
    return name, value


def decode(lines, traces):
    groups, order = {}, []
    for line in lines:
        start = line.find(PREFIX)
        if start < 0:
            continue
        try:
            row = json.loads(line[start:])
        except json.JSONDecodeError:
            continue
        key = (words_hex(row["daemon"]), words_hex(row["scope"]), row["call"])
        if key not in groups:
            groups[key] = {
                "daemon": key[0],
                "scope": key[1],
                "call": key[2],
                "slot": row["slot"],
                "complete": False,
                "sections": {},
            }
            order.append(key)
        group = groups[key]
        name, value = named(row["section"], row["index"], row["available"], row["values"])
        group["sections"][name] = value
        if row["section"] == 0 and value:
            group["operation"] = value["operation"]
        if row["section"] == 99:
            group["complete"] = True
    for key in order:
        group = groups[key]
        if group["scope"] in traces:
            group["trace"] = traces[group["scope"]]
        yield group


def trace_scopes(path):
    scopes = {}
    with open(path, encoding="utf-8") as lines:
        for line in lines:
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            if entry.get("scope"):
                scopes[entry["scope"]] = entry["id"]
    return scopes


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("logs", nargs="?", help="log file (default: standard input)")
    parser.add_argument("--trace", help="SDK trace file (LAYERFS_TRACE) to join by scope")
    args = parser.parse_args()
    traces = trace_scopes(args.trace) if args.trace else {}
    source = open(args.logs, encoding="utf-8", errors="replace") if args.logs else sys.stdin
    with source:
        for group in decode(source, traces):
            print(json.dumps(group, separators=(",", ":")))


if __name__ == "__main__":
    main()
