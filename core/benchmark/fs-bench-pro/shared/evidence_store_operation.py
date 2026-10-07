"""Whole Store-half Commit accounting, without timing or physical-I/O promotion."""
from .evidence_engine import rows
from .evidence_jobs import (ALLOCATION, PAYLOAD, SHA, STATEMENT, counter_map, database,
                            integer, require)

SQL = ("statements", "vm_steps", "fullscan_steps", "sorts", "autoindex_rows", "reprepares",
       "returned_rows", "bound_bytes", "statement_ns", "commit_ns", "transactions", "write_transactions",
       "write_commits", "commits", "read_snapshots", "rollbacks", "transaction_ns", "read_snapshot_ns",
       "sealed_inserts", "sealed_body_bytes", "blob_open_calls", "blob_reopen_calls", "blob_read_calls",
       "blob_requested_bytes", "blob_read_bytes", "blob_read_ns", "blob_close_calls")
STORAGE = ("policy", "locate", "read_packs", "read_pack_selections", "whole_selected",
           "whole_due_density", "whole_due_singleton", "whole_due_small", "whole_due_reuse",
           "range_selected", "range_scan_bytes", "range_acquired_bytes", "range_materialized_bytes",
           "value_groups", "signatures", "reserve", "initial_reservations", "reservation_refills",
           "publish", "payload_reads", "payload_read_bytes", "pack_read_bytes", "pack_write_bytes",
           "pooled_packs", "pooled_groups", "reserved_directory_bytes", "ordinal_reservations",
           "locator_hits", "locator_misses", "locator_evictions", "locator_eviction_probes",
           "locator_second_chances", "locator_bookkeeping_live_peak_bytes", "pack_hits",
           "pack_misses", "pack_evictions", "pack_evicted_bytes", "directory_validations",
           "directory_unshared_walks", "directory_entry_bounds", "prefetch_group_decodes", "forced_seals")


def sql(value):
    require(set(value) == set(SQL) | {"statement_phases", "commit_phases"}, "exact Store SQL inventory")
    require(all(integer(value[key]) for key in SQL), "Store SQL counter types")
    for key in ("statement_phases", "commit_phases"):
        require(len(value[key]) == 7, "Store wrapper phase inventory")
        for phase in value[key]:
            counter_map(phase, ("calls", "wall_ns"), "Store wrapper phase")
    require(value["blob_read_bytes"] <= value["blob_requested_bytes"], "Store BLOB byte scope")


def validate(path):
    results = []
    original = None
    for index, row in enumerate(rows(path)):
        require(index < 2 and row.get("schema") == "cluster-two-store-accounting-v1" and row.get("mode") == "diagnostic", "Store operation schema/order")
        require(row.get("attempt_count") == 1 and type(row.get("attempt_count")) is int, "one original Store attempt")
        require(row.get("namespace_files") == 100000, "whole prepared namespace selection")
        require(row.get("outcome") == ("Committed", "UpToDate")[index], "known original Store outcome")
        require(isinstance(row.get("root"), str) and SHA.fullmatch(row["root"]) and
                isinstance(row.get("head"), str) and row["head"].startswith("12") and
                SHA.fullmatch(row["head"][2:]),
                "known root/head identities")
        identity = row["root"], row["head"]
        require(original is None or original == identity, "UpToDate changed the published root/head")
        original = identity
        for name in ("engine_capture", "engine_install", "engine_whole_commit"):
            database(row[name])
        for family in range(14):
            require(all(row["engine_whole_commit"]["families"][family][key] ==
                        row["engine_capture"]["families"][family][key] + row["engine_install"]["families"][family][key]
                        for key in STATEMENT), "whole Commit engine family accounting")
        counter_map(row["engine_payload"], PAYLOAD, "whole Commit payload")
        counter_map(row["engine_allocation"], ALLOCATION, "whole Commit allocation")
        sql(row["store_writer"])
        require(len(row["store_readers"]) == 4, "fixed reader inventory")
        for reader in row["store_readers"]:
            sql(reader)
        storage = counter_map(row["storage"], STORAGE, "Storage counters")
        require(storage["initial_reservations"] == 1 and storage["reservation_refills"] == 0 and
                storage["reserve"] == 1, "one initial reservation in this small Save")
        require(row["store_writer"]["write_commits"] == storage["reserve"] + storage["publish"] + 1,
                "Save batches plus one history transaction")
        require(storage["signatures"] == 1, "fixed signature-ring load count")
        require(row["store_writer"]["fullscan_steps"] == storage["signatures"] * 8191,
                "unexplained scan outside the saturated8192-slot signature ring")
        require(all(reader["write_transactions"] == 0 for reader in row["store_readers"]), "read handle write")
        results.append({"outcome": row["outcome"], "write_transactions": row["store_writer"]["write_commits"],
                        "statements": row["store_writer"]["statements"] + sum(r["statements"] for r in row["store_readers"]),
                        "engine_statements": row["engine_whole_commit"]["total"]["attempts"],
                        "pack_write_bytes": storage["pack_write_bytes"],
                        "bounded_signature_scan_steps": row["store_writer"]["fullscan_steps"],
                        "unexplained_scan_steps": 0})
    require(len(results) == 2, "incomplete original Store operation inventory")
    return {"schema": "pre-s8-store-operation-validation-v1", "functional_count_status": "PASS",
            "operations": results, "numeric_acceptance": "OWNER_DEFERRED",
            "limitations": ["Scope is the Store half with direct Content construction, not S10 normalization",
                            "Counts are logical calls/rows/bytes; no physical-I/O or phase-peak inference"]}
