"""Whole Store-half Commit accounting, without timing or physical-I/O promotion."""
from .evidence_engine import rows
from .evidence_jobs import (ALLOCATION, PAYLOAD, STATEMENT, counter_map, database,
                            integer, require)

SQL = ("statements", "vm_steps", "fullscan_steps", "sorts", "autoindex_rows", "reprepares",
       "returned_rows", "bound_bytes", "statement_ns", "commit_ns", "transactions", "write_transactions",
       "write_commits", "commits", "read_snapshots", "rollbacks", "transaction_ns", "read_snapshot_ns",
       "sealed_inserts", "sealed_body_bytes", "blob_open_calls", "blob_reopen_calls", "blob_read_calls",
       "blob_requested_bytes", "blob_read_bytes", "blob_read_ns", "blob_close_calls")


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
    for index, row in enumerate(rows(path)):
        require(index < 2 and row.get("schema") == "cluster-two-store-accounting-v1" and row.get("mode") == "diagnostic", "Store operation schema/order")
        require(row.get("attempt_count") == 1 and type(row.get("attempt_count")) is int, "one original Store attempt")
        require(row.get("namespace_files") == 100000, "whole prepared namespace selection")
        require(row.get("outcome") == ("Committed", "UpToDate")[index], "known original Store outcome")
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
        storage = row["storage"]
        require(all(integer(value) for value in storage.values()), "Storage counter types")
        require(storage["initial_reservations"] == 1 and storage["reservation_refills"] == 0 and
                storage["reserve"] == 1, "one initial reservation in this small Save")
        require(row["store_writer"]["write_commits"] == storage["reserve"] + storage["publish"] + 1,
                "Save batches plus one history transaction")
        require(row["store_writer"]["fullscan_steps"] == 0, "incremental Store scan")
        require(all(reader["write_transactions"] == 0 for reader in row["store_readers"]), "read handle write")
        results.append({"outcome": row["outcome"], "write_transactions": row["store_writer"]["write_commits"],
                        "statements": row["store_writer"]["statements"] + sum(r["statements"] for r in row["store_readers"]),
                        "engine_statements": row["engine_whole_commit"]["total"]["attempts"],
                        "pack_write_bytes": storage["pack_write_bytes"]})
    require(len(results) == 2, "incomplete original Store operation inventory")
    return {"schema": "pre-s8-store-operation-validation-v1", "functional_count_status": "PASS",
            "operations": results, "numeric_acceptance": "OWNER_DEFERRED",
            "limitations": ["Scope is the Store half with direct Content construction, not S10 normalization",
                            "Counts are logical calls/rows/bytes; no physical-I/O or phase-peak inference"]}
