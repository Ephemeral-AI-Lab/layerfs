"""Source-pinned numeric counter views and exact observer-count subtraction.

Schema: core/docs/architecture/70-daemon-application-startup-control.md.
Array order: application/diagnostics/schema.rs and resources.rs. This module
accepts explicit original endpoints; it never invents phase pairs from Debug,
wall time or neighboring calls. SDK spans remain instrumented complete spans.
"""
import argparse
import json
from pathlib import Path

SCHEMA_SOURCE = "core/docs/architecture/70-daemon-application-startup-control.md"
STATEMENT = ("attempts", "executions", "rows_returned", "rows_changed", "direct_rows_changed",
             "returned_blob_bytes", "returned_value_bytes", "vm_steps", "fullscan_steps", "sorts",
             "autoindex_rows", "reprepares", "bound_bytes", "sql_bytes", "statement_memory_samples",
             "statement_memory_sample_bytes", "elapsed_ns")
PAYLOAD = ("write_input_bytes", "write_cells", "partial_write_cells", "cell_copy_bytes", "cell_zeroed_bytes",
           "read_window_zeroed_bytes", "read_local_copy_bytes")
ALLOCATION = ("attempts", "requested_bytes", "admitted_jobs", "refusals", "observations", "freelist_queries")
OWNER_SCALARS = ("admitted", "credited_bytes", "peak_credited_bytes", "outstanding", "scheduler_bytes",
                 "queued", "peak_queued", "receipt_overruns", "receipt_overrun_bytes", "maintenance_jobs",
                 "maintenance_rows", "closed_namespaces", "maintenance_data_bytes", "maintenance_ns")
CLASSES = ("Read", "Mutation", "Capture", "Lifecycle", "OperationRecord", "Source")
STORED = ("wait_refs", "namespaces", "inode_rows", "directory_entry_rows", "payload_cells", "payload_bytes",
          "shrink_rows", "operation_record_rows", "operation_record_bytes", "orphan_rows", "owner_rows",
          "source_rows", "owner_details", "reply_tickets", "retire_targets", "maintenance_targets", "ready_targets")
ALLOCATION_STATE = ("logical_bytes", "allocated_bytes", "high_water_allocated_bytes", "reserved_tail_bytes", "cleanup_headroom_bytes")
STORAGE = ("policy", "locate", "read_packs", "read_pack_selections", "whole_selected", "whole_due_density",
           "whole_due_singleton", "whole_due_small", "whole_due_reuse", "range_selected", "range_scan_bytes",
           "range_acquired_bytes", "range_materialized_bytes", "value_groups", "signatures", "reserve",
           "initial_reservations", "reservation_refills", "publish", "payload_reads", "payload_read_bytes",
           "pack_read_bytes", "pack_write_bytes", "pooled_packs", "pooled_groups", "reserved_directory_bytes",
           "ordinal_reservations", "locator_hits", "locator_misses", "locator_evictions", "locator_eviction_probes",
           "locator_second_chances", "locator_bookkeeping_live_peak_bytes", "pack_hits", "pack_misses",
           "pack_evictions", "pack_evicted_bytes", "directory_validations", "directory_unshared_walks",
           "directory_entry_bounds", "prefetch_group_decodes", "forced_seals")
STORE = ("object_batches", "object_ids", "length_batches", "length_ids", "serial_reservations")
READER = ("stopping", "poisoned", "readers", "quarantined", "waiting", "assigned", "leased", "outstanding",
          "peak_outstanding", "grants", "queue_wait_ns", "maximum_queue_wait_ns", "scheduler_bytes")
CACHE = ("cache_hits", "cache_misses", "upstream_batches", "authenticated_bytes", "evictions", "charged_cache_bytes", "cached_objects")
CONSTRUCTION = ("inode_rows", "entry_rows", "inode_pages", "entry_pages", "change_pages", "largest_page",
                "parent_points", "base_lookups", "headers_written", "headers_dropped", "values_written",
                "fresh_serials", "tombstones_skipped", "files_constructed", "files_unchanged", "symlinks",
                "metadata_built", "metadata_patched", "record_jobs", "header_opens", "header_rows", "header_points",
                "change_opens", "change_rows", "change_points", "value_opens", "value_rows", "value_points",
                "fresh_opens", "fresh_rows", "fresh_points")
GLOBAL_SQL = ("statements", "vm_steps", "fullscan_steps", "sorts", "autoindex_rows", "reprepares", "returned_rows",
              "bound_bytes", "statement_ns", "commit_ns", "transactions", "write_transactions", "write_commits",
              "commits", "read_snapshots", "rollbacks", "transaction_ns", "read_snapshot_ns", "sealed_inserts",
              "sealed_body_bytes", "blob_open_calls", "blob_reopen_calls", "blob_read_calls", "blob_requested_bytes",
              "blob_read_bytes", "blob_read_ns", "blob_close_calls")
DISPATCH = ("configured_workers", "entered_workers", "live_workers", "mounts", "queued", "running", "parked", "retained", "stopping", "failed")
NATIVE = ("owner_id", "root_serial", "serving", "detached", "abort_bound")
LOOPS = ("phase", "revision", "configured", "created", "entered", "exited", "joined")
MOUNT = ("received", "admitted", "queued", "running", "parked", "retained", "owned_input_bytes", "future_bytes",
         "receive_units", "completed", "steps", "wakes", "terminal")
OPCODE_DISPOSITION = ("handoffs", "inline", "refused", "terminal", "unadmitted", "forget_units", "store_units")
OPERATIONS = {"mount": (1,9), "commit": (2,), "workspace_status": (3,), "unmount": (4,), "cleanup": (13,)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identity(group):
    header = group[0]
    return (tuple(header["daemon"]), tuple(header["scope"]), header["call"])


def names(values, keys):
    require(len(values) == len(keys), "source counter schema cardinality mismatch")
    require(all(type(value) is int and 0 <= value < 2**64 for value in values), "source counter values must be u64")
    return dict(zip(keys, values))


def row(group, section, index=0):
    found = [item for item in group if item["section"] == section and item["index"] == index]
    require(len(found) <= 1, "duplicate original counter row")
    if not found or not found[0]["available"]:
        return None
    return found[0]["values"]


def decode(group):
    result = {"identity": identity(group), "operation_tag": group[0]["values"][0],
              "namespace": group[0]["index"], "schema_source": SCHEMA_SOURCE,
              "field_availability": {}}
    scalar = row(group, 1)
    if scalar is not None:
        require(len(scalar) == 32, "owner scalar/class schema cardinality")
        result["owner"] = names(scalar[:14], OWNER_SCALARS)
        for key, offset in (("completed",14), ("queue_wait_ns",20), ("service_ns",26)):
            result["owner"][key] = names(scalar[offset:offset+6], CLASSES)
    for key, section, fields in (("store",8,STORE), ("reader",9,READER), ("cache",10,CACHE),
                                 ("construction",15,CONSTRUCTION), ("storage",16,STORAGE), ("dispatch",11,DISPATCH),
                                 ("native",12,NATIVE), ("loops",24,LOOPS), ("mount",25,MOUNT),
                                 ("payload_foreground",4,PAYLOAD), ("payload_maintenance",5,PAYLOAD),
                                 ("allocation_foreground",6,ALLOCATION), ("allocation_maintenance",7,ALLOCATION)):
        values = row(group, section)
        if values is not None:
            result[key] = names(values, fields)
        result["field_availability"][key] = "AVAILABLE" if values is not None else "UNAVAILABLE"
    for key, section in (("sql_foreground",2), ("sql_maintenance",3)):
        families = [row(group, section, index) for index in range(14)]
        if all(values is not None for values in families):
            result[key] = [names(values, STATEMENT) for values in families]
        result["field_availability"][key] = "AVAILABLE" if key in result else "UNAVAILABLE"
    values = row(group, 28)
    if values is not None:
        require(len(values) == 31, "resource schema cardinality")
        result["stored_counts"] = names(values[:17], STORED)
        result["allocation_state"] = names(values[17:22], ALLOCATION_STATE)
        result["allocation_work"] = names(values[22:28], ALLOCATION)
        result["pages"] = names(values[28:], ("database_pages", "free_pages", "debt_upper_bytes"))
    result["reader_storage"] = {item["index"]: names(item["values"], STORAGE) for item in group
                                if item["section"] == 29 and item["available"]}
    opcode = row(group,26)
    if opcode is not None:
        require(len(opcode) == 46, "opcode/disposition source schema cardinality")
        result["opcodes"] = {"by_opcode": opcode[:39], "disposition": names(opcode[39:],OPCODE_DISPOSITION)}
    for section, field in ((13,"global_sql_writer"),(14,"global_sql_readers")):
        selected = {}
        for item in group:
            if item["section"] != section or not item["available"]:
                continue
            values = item["values"]
            require(len(values) == 55, "global SQL scalar/phase source schema cardinality")
            work = names(values[:27],GLOBAL_SQL)
            work["statement_phases"] = [names(values[27+2*index:29+2*index],("calls","wall_ns")) for index in range(7)]
            work["commit_phases"] = [names(values[41+2*index:43+2*index],("calls","wall_ns")) for index in range(7)]
            selected[item["index"]] = work
        if selected:
            result[field] = selected
    cost = row(group, 30)
    if cost is not None:
        require(len(cost) in {2,35} and cost[0] in {0,1} and cost[1] in {0,1}, "observer cost schema cardinality/bits")
        result["observer"] = {"admitted": bool(cost[0]), "completion_available": bool(cost[1])}
        if cost[1]:
            require(len(cost) == 35 and cost[0] == 1, "completed observer work requires original admission")
            result["observer"].update(sql=names(cost[2:19], STATEMENT), payload=names(cost[19:26], PAYLOAD),
                                      allocation=names(cost[26:32], ALLOCATION),
                                      times=names(cost[32:], ("parked_turns", "queue_wait_ns", "service_ns")))
        else:
            require(len(cost) == 2, "unavailable observer work must not fabricate counter arrays")
    return result


def correlate(events, groups):
    """Exact successful control record ranges; failure ranges stay unavailable."""
    origins = [event for event in events if event.get("event") == "control_origin"]
    require(len(origins) == 1 and origins[0].get("fields", {}).get("control_send_records") == 1,
            "fresh serve Control origin must establish exactly one Hello")
    decoded = {identity(group): decode(group) for group in groups.values()}
    require(len(decoded) == len(groups), "duplicate original daemon/scope/call identity")
    mapped = set()
    correlation = []
    previous = 1
    for event in events:
        codes = OPERATIONS.get(event.get("event"))
        if codes is None:
            continue
        fields = event.get("fields", {})
        before, after = fields.get("control_send_records_before"), fields.get("control_send_records_after")
        require(type(before) is int and type(after) is int and before == previous and after-before == len(codes),
                "successful facade control range differs from original completed send records")
        selected = []
        for call, code in zip(range(before+1,after+1), codes):
            matches = [(key,value) for key,value in decoded.items() if key[2] == call]
            require(len(matches) == 1, "original numeric group missing or cross-scope call ambiguity")
            key, value = matches[0]
            require(value["operation_tag"] == code and key not in mapped, "original call operation mismatch or duplicate mapping")
            mapped.add(key)
            selected.append(key)
        correlation.append({"event_sequence": event["sequence"], "event": event["event"],
                            "before_records": before, "after_records": after, "identities": selected,
                            "span_scope": "instrumented complete SDK; observer acquisition/format/transport included"})
        if event["event"] == "workspace_status":
            available = fields.get("status_state_attribution_available")
            jobs = fields.get("status_state_lifecycle_jobs")
            require(available is None or type(available) is int and available in {0,1}, "typed Status State attribution bit invalid")
            if available == 1:
                require(type(jobs) is int and jobs == 1, "typed original Status local Some requires exactly one State Lifecycle job")
                correlation[-1]["status_state"] = {"status": "AVAILABLE", "Lifecycle_jobs": jobs,
                            "provenance": "original typed Status.local Some; actual State SQL VM work unavailable"}
            else:
                require(jobs is None, "unavailable typed Status state must not fabricate job count")
                correlation[-1]["status_state"] = {"status": "UNAVAILABLE", "reason": "original local outcome missing or unavailable; no retrospective inference"}
        previous = after
    require(mapped == set(decoded), "unmapped numeric groups; never infer successful failure calls")
    return correlation


def difference(before, end, keys):
    result = {}
    for key in keys:
        left, right = before[key], end[key]
        require(left != 2**64-1 and right != 2**64-1 and right >= left, "cumulative counter decreased or saturated: " + key)
        result[key] = right-left
    return result


def resolve_selectors(selectors, bindings, correlation):
    """Resolve prospective logical endpoints against original successful ranges."""
    by_sequence = {item["event_sequence"]:item for item in correlation}
    require(len(by_sequence) == len(correlation), "duplicate original correlated event sequence")
    result = []
    for selected in selectors:
        resolved = {"label":selected["label"],"selectors":selected,"status":"UNAVAILABLE"}
        try:
            for role in ("before","end"):
                binding = bindings[selected[role]]
                original = by_sequence[binding["event_sequence"]]
                require(original["event"] == binding["event"], "original endpoint event type mismatch")
                position = binding["position"]
                require(position in {"first","last"}, "endpoint position must select first/last original successful call")
                identities = original["identities"]
                require(identities, "original endpoint has no successful numeric group")
                resolved[role+"_identity"] = identities[0 if position == "first" else -1]
                resolved[role+"_original"] = binding
            resolved["status"] = "RESOLVED"
        except (KeyError,ValueError,TypeError,IndexError) as error:
            resolved["reason"] = str(error)
        result.append(resolved)
    return result


def phase_counts(groups, before_identity, end_identity, *, exclusive_observer_scope=False, label="explicit interval", control_correlation=None):
    """Count-only subtraction over caller-selected original snapshot endpoints."""
    result = {"schema": "r7-phase-counts-v1", "status": "UNAVAILABLE", "label": label,
              "before_identity": before_identity, "end_identity": end_identity,
              "attribution_scope": "daemon-shared; concurrency not attributed to one Workspace",
              "time_scope": "instrumented; no formatter/transport/wait/service time subtraction",
              "gauges_and_peaks": "absolute original snapshots; never subtracted",
              "observer_credit": "caller Completion drop does not prove publisher Arc/credit already released"}
    try:
        require(exclusive_observer_scope, "observer channel exclusivity unavailable; other observers may affect cumulative counts")
        before_identity = (tuple(before_identity[0]), tuple(before_identity[1]), before_identity[2])
        end_identity = (tuple(end_identity[0]), tuple(end_identity[1]), end_identity[2])
        require(before_identity[:2] == end_identity[:2] and before_identity[2] < end_identity[2], "endpoint daemon/scope/order mismatch")
        snapshots = {identity(group): decode(group) for group in groups.values()}
        require(len(snapshots) == len(groups), "duplicate original snapshot identity")
        before, end = snapshots[before_identity], snapshots[end_identity]
        result["original_snapshots"] = {"before": before, "end": end}
        require("owner" in before and "owner" in end and "sql_foreground" in before and "sql_foreground" in end,
                "original endpoint owner/SQL snapshot unavailable")
        interval = [value for key,value in snapshots.items() if key[:2] == before_identity[:2] and before_identity[2] < key[2] <= end_identity[2]]
        require(len(interval) == end_identity[2]-before_identity[2], "original interval observer call missing")
        require(all(value.get("observer", {}).get("completion_available") for value in interval), "exact observer work unavailable in interval")
        statement_keys = tuple(key for key in STATEMENT if key != "elapsed_ns")
        raw_owner = difference(before["owner"], end["owner"], ("admitted", "receipt_overruns", "receipt_overrun_bytes",
                               "maintenance_jobs", "maintenance_rows", "closed_namespaces", "maintenance_data_bytes"))
        raw_classes = difference(before["owner"]["completed"], end["owner"]["completed"], CLASSES)
        raw_sql = [difference(left,right,statement_keys) for left,right in zip(before["sql_foreground"],end["sql_foreground"])]
        observer_sql = {key: sum(value["observer"]["sql"][key] for value in interval) for key in statement_keys}
        observer_jobs = len(interval)
        require(raw_owner["admitted"] >= observer_jobs and raw_classes["Read"] >= observer_jobs, "observer jobs exceed original cumulative delta")
        adjusted_owner, adjusted_classes = dict(raw_owner), dict(raw_classes)
        adjusted_owner["admitted"] -= observer_jobs
        adjusted_classes["Read"] -= observer_jobs
        adjusted_sql = [dict(values) for values in raw_sql]
        for key in statement_keys:
            require(adjusted_sql[0][key] >= observer_sql[key], "observer Startup work exceeds original family delta: " + key)
            adjusted_sql[0][key] -= observer_sql[key]
        result.update(status="AVAILABLE", raw={"owner": raw_owner, "completed": raw_classes, "sql_foreground": raw_sql},
                      observer={"jobs": observer_jobs, "Read_completions": observer_jobs, "Startup_statement_work": observer_sql,
                                "original_costs": [value["observer"] for value in interval]},
                      adjusted={"owner": adjusted_owner, "completed": adjusted_classes, "sql_foreground": adjusted_sql})
        status_calls = {value["identity"] for value in interval if value["operation_tag"] == 3}
        states = {tuple((tuple(key[0]),tuple(key[1]),key[2])): item.get("status_state", {})
                  for item in (control_correlation or []) if item["event"] == "workspace_status" for key in item["identities"]}
        if all(states.get(key, {}).get("status") == "AVAILABLE" for key in status_calls):
            jobs = sum(states[key]["Lifecycle_jobs"] for key in status_calls)
            require(adjusted_owner["admitted"] >= jobs and adjusted_classes["Lifecycle"] >= jobs,
                    "typed Status State jobs exceed original post-Resources cumulative delta")
            without_status_owner, without_status_classes = dict(adjusted_owner), dict(adjusted_classes)
            without_status_owner["admitted"] -= jobs
            without_status_classes["Lifecycle"] -= jobs
            result["status_state_attribution"] = {"status": "AVAILABLE", "Lifecycle_jobs": jobs,
                        "original_operands": [states[key] for key in sorted(status_calls)],
                        "sql_work": "UNAVAILABLE: no exact original State StatementWork supplied"}
            result["adjusted_without_status_state_jobs"] = {"owner": without_status_owner, "completed": without_status_classes,
                        "scope": "count-only floor view; original public Status jobs retained in raw and Resources-adjusted views; SQL/time unchanged"}
        else:
            result["status_state_attribution"] = {"status": "UNAVAILABLE", "reason": "typed original Status.local outcome unavailable; no guessed job/SQL subtraction"}
        for key, fields in (("store",STORE),("cache",CACHE[:5])):
            if key in before and key in end:
                result["raw"][key] = difference(before[key],end[key],fields)
        if "reader" in before and "reader" in end:
            result["raw"]["reader"] = difference(before["reader"],end["reader"],("grants",))
        if "native" in before and "native" in end and before["native"]["owner_id"] == end["native"]["owner_id"] and "opcodes" in before and "opcodes" in end:
            left, right = before["opcodes"], end["opcodes"]
            require(all(last >= first and last != 2**64-1 and first != 2**64-1 for first,last in zip(left["by_opcode"],right["by_opcode"])), "opcode counters decreased or saturated")
            result["raw"]["opcodes"] = {"by_opcode": [last-first for first,last in zip(left["by_opcode"],right["by_opcode"])],
                    "disposition": difference(left["disposition"],right["disposition"],OPCODE_DISPOSITION),
                    "scope": "one original mount connection; control diagnostic counts are not FUSE requests"}
    except (ValueError, KeyError, TypeError, IndexError) as error:
        result["status"] = "UNAVAILABLE"
        result["reason"] = str(error)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--numeric-validation", type=Path, required=True)
    parser.add_argument("--before-identity", required=True, help="JSON [daemon words, scope words, call]")
    parser.add_argument("--end-identity", required=True, help="JSON [daemon words, scope words, call]")
    parser.add_argument("--exclusive-observer-scope", action="store_true")
    parser.add_argument("--control-correlation", type=Path, help="original validated successful send ranges with typed Status attribution")
    parser.add_argument("--label", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    original = json.loads(args.numeric_validation.read_text())
    require(original.get("status") == "PASS", "complete validated original numeric framing required")
    correlation = json.loads(args.control_correlation.read_text())["calls"] if args.control_correlation else None
    result = phase_counts(original["sections"], json.loads(args.before_identity), json.loads(args.end_identity),
                          exclusive_observer_scope=args.exclusive_observer_scope, label=args.label, control_correlation=correlation)
    with args.output.open("x") as stream:
        json.dump(result,stream,indent=2,sort_keys=True)
        stream.write("\n")
    print(json.dumps({"status": result["status"], "output": str(args.output), "reason": result.get("reason")}))


if __name__ == "__main__":
    main()
