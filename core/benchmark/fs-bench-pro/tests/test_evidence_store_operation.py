"""Tamper and omission checks over two original known Store outcomes."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_store_operation as store

ROOT = Path(__file__).resolve().parents[4]
FIXTURE = ROOT / "core/docs/issues/307/checks/pre-s8-accounting-20261007/54-host-store-v2/jobs.jsonl"

class StoreReceipts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = list(store.rows(FIXTURE))

    def rejected(self, edit, reason):
        values = copy.deepcopy(self.original)
        edit(values)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "rows.jsonl"
            path.write_text("".join(json.dumps(r) + "\n" for r in values))
            with self.assertRaisesRegex(ValueError, reason):
                store.validate(path)

    def test_actual_known_pair_counts_only(self):
        result = store.validate(FIXTURE)
        self.assertEqual(result["functional_count_status"], "PASS")
        self.assertEqual([r["write_transactions"] for r in result["operations"]], [3, 3])
        self.assertEqual([r["pack_write_bytes"] for r in result["operations"]], [8292, 0])
        self.assertEqual(result["numeric_acceptance"], "OWNER_DEFERRED")

    def test_missing_operation(self):
        self.rejected(lambda r: r.pop(), "incomplete original")

    def test_duplicate_operation(self):
        self.rejected(lambda r: r.append(copy.deepcopy(r[1])), "schema/order")

    def test_wrong_result(self):
        self.rejected(lambda r: r[1].__setitem__("outcome", "Committed"), "known original")

    def test_root_changed(self):
        self.rejected(lambda r: r[1].__setitem__("root", "f" * 64), "UpToDate changed")

    def test_missing_root(self):
        self.rejected(lambda r: r[0].pop("root"), "root/head")

    def test_wrong_history_tag(self):
        self.rejected(lambda r: r[0].__setitem__("head", "13" + r[0]["head"][2:]), "root/head")

    def test_second_attempt(self):
        self.rejected(lambda r: r[0].__setitem__("attempt_count", 2), "one original")

    def test_missing_storage_counter(self):
        self.rejected(lambda r: r[0]["storage"].pop("pack_read_bytes"), "exact counter inventory")

    def test_unexplained_scan(self):
        self.rejected(lambda r: r[0]["store_writer"].__setitem__("fullscan_steps", 8192), "unexplained scan")

    def test_hidden_ring_load(self):
        self.rejected(lambda r: r[1]["storage"].__setitem__("signatures", 0), "ring load")

    def test_extra_transaction(self):
        self.rejected(lambda r: r[0]["store_writer"].__setitem__("write_commits", 4), "history transaction")

    def test_missing_reader(self):
        self.rejected(lambda r: r[0]["store_readers"].pop(), "reader inventory")

    def test_reader_write(self):
        self.rejected(lambda r: r[0]["store_readers"][0].__setitem__("write_transactions", 1), "read handle write")

    def test_missing_sql_counter(self):
        self.rejected(lambda r: r[0]["store_writer"].pop("blob_close_calls"), "SQL inventory")

    def test_family_omission(self):
        self.rejected(lambda r: r[0]["engine_whole_commit"]["families"].pop(), "family cardinality")

    def test_whole_operation_undercharge(self):
        def edit(r):
            r[0]["engine_whole_commit"]["families"][0]["vm_steps"] += 1
            r[0]["engine_whole_commit"]["total"]["vm_steps"] += 1
        self.rejected(edit, "whole Commit engine family")

if __name__ == "__main__":
    unittest.main()
