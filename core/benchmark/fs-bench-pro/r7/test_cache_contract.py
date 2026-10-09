"""External cache-envelope refusals; no residency or product execution."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import receipts, registry, test_registry


def scalar():
    files = []
    for database in ("/layerfs-store/global/store.sqlite", "/layerfs-local/overlay/overlay.sqlite"):
        files.append(dict(path=database,present=True,resident_pages=0,eviction_hint_attempts=1))
        files.extend(dict(path=database+suffix,present=False,resident_pages=0) for suffix in ("-wal","-shm","-journal"))
    return dict(**{"class":"A"}, scopes=dict.fromkeys(registry.PHASES,"synthetic declared scope; never cold proof"),
                fresh_daemon_cache=True,fresh_kernel_connection=True,store_path=files[0]["path"],overlay_path=files[4]["path"],
                required_residency_paths=[row["path"] for row in files],residency=dict(cache_class="A",status="ELIGIBLE",files=files,
                    resident_pages=0,payload_bytes_read=0,attempts=0,method="one fadvise hint then mincore"))


def streamed():
    return {"class":"A","scopes":dict.fromkeys(registry.PHASES,"synthetic file-only scope"),"fresh_kernel_connection":True,
            "stream_inventory_artifact":"original.jsonl","stream_inventory_sha256":"a"*64,
            "residency":dict(schema="r7-residency-stream-v1",cache_class="A",status="ELIGIBLE",resident_pages=0,
                eviction_hint_attempts=1,payload_bytes_read=0,attempts=0,method="fadvise mincore",
                inventory=dict(created=True,complete=True,sha256="a"*64,rows=1,physical_files=1),
                input_manifest=dict(root="/native",sha256="b"*64,expected_sha256="b"*64,declared_files=1,declared_physical_files=1))}


class CacheContract(unittest.TestCase):
    def test_optional_absent_sidecars_and_present_databases_remain_valid(self):
        row = scalar()
        receipts.validate_cache(row,"L",1)
        self.assertEqual(sum(not item["present"] for item in row["residency"]["files"]),6)

    def test_present_cold_sidecars_require_the_original_hint(self):
        row = scalar()
        sidecar = row["residency"]["files"][1]
        sidecar.update(present=True,eviction_hint_attempts=1)
        receipts.validate_cache(row,"L",1)
        sidecar["eviction_hint_attempts"] = 0
        with self.assertRaisesRegex(ValueError,"eviction and measured residency"):
            receipts.validate_cache(row,"L",1)

    def test_main_missing_unknown_failed_pending_or_unavailable_status_refuses(self):
        for status in (None,"UNAVAILABLE","FAILED","PENDING_PHASE_COUNTERS","PENDING_WARMUP_BOUNDARY"):
            row = scalar()
            if status is None:
                del row["residency"]["status"]
            else:
                row["residency"]["status"] = status
            with self.subTest(status=status),self.assertRaisesRegex(ValueError,"main cold predicate"):
                receipts.validate_cache(row,"L",1)

    def test_main_stream_unavailable_refuses_despite_complete_zero_page_footer(self):
        for arm in ("N","P"):
            row = streamed()
            row["residency"]["status"] = "UNAVAILABLE"
            with self.subTest(arm=arm),self.assertRaisesRegex(ValueError,"main cold predicate"):
                receipts.validate_cache(row,arm,1)

    def test_mandatory_store_and_overlay_presence_cannot_be_false(self):
        for position in (0,4):
            row = scalar()
            row["residency"]["files"][position]["present"] = False
            with self.subTest(position=position),self.assertRaisesRegex(ValueError,"mandatory Store and overlay"):
                receipts.validate_cache(row,"L",1)

    def test_missing_overlay_declaration_or_sidecar_population_refuses(self):
        row = scalar()
        del row["overlay_path"]
        with self.assertRaisesRegex(ValueError,"overlay path"):
            receipts.validate_cache(row,"L",1)
        row = scalar()
        row["residency"]["files"] = row["residency"]["files"][:4]
        row["required_residency_paths"] = [item["path"] for item in row["residency"]["files"]]
        with self.assertRaisesRegex(ValueError,"complete input and sidecar"):
            receipts.validate_cache(row,"L",1)

    def test_store_and_overlay_cannot_alias_one_declared_path(self):
        row = scalar()
        row["overlay_path"] = row["store_path"]
        with self.assertRaisesRegex(ValueError,"must be distinct"):
            receipts.validate_cache(row,"L",1)

    def test_false_presence_positive_residency_refuses_even_zero_attempt_ineligible(self):
        row = scalar()
        row["residency"]["files"][1]["resident_pages"] = 1
        row["residency"].update(status="INELIGIBLE",resident_pages=1)
        with self.assertRaisesRegex(ValueError,"absent input cannot have resident pages"):
            receipts.validate_cache(row,"L",0,True)

    def test_actual_resident_sidecar_is_retained_ineligible_with_zero_attempts(self):
        row = scalar()
        row["residency"]["files"][1].update(present=True,resident_pages=1,eviction_hint_attempts=1)
        row["residency"].update(status="INELIGIBLE",resident_pages=1)
        original = copy.deepcopy(row)
        receipts.validate_cache(row,"L",0,True)
        self.assertEqual(row,original)
        with self.assertRaisesRegex(ValueError,"resident cold input attempted"):
            receipts.validate_cache(row,"L",1,True)
        with self.assertRaisesRegex(ValueError,"ineligible disposition"):
            receipts.validate_cache(row,"L",0,False)

    def test_scalar_eligible_nonzero_or_ineligible_zero_pages_refuse(self):
        for status,pages in (("ELIGIBLE",1),("INELIGIBLE",0)):
            row = scalar()
            row["residency"]["files"][0]["resident_pages"] = pages
            row["residency"].update(status=status,resident_pages=pages)
            with self.subTest(status=status),self.assertRaisesRegex(ValueError,"status and observed residency disagree"):
                receipts.validate_cache(row,"L",0,True)

    def test_streamed_native_and_passthrough_cardinality_and_ineligible_disposition(self):
        for arm in ("N","P"):
            row = streamed()
            receipts.validate_cache(row,arm,1)
            row["residency"].update(status="INELIGIBLE",resident_pages=1)
            receipts.validate_cache(row,arm,0,True)
            with self.assertRaisesRegex(ValueError,"resident cold input attempted"):
                receipts.validate_cache(row,arm,1,True)
            row["residency"].update(status="ELIGIBLE",resident_pages=0)
            row["residency"]["inventory"]["rows"] = 2
            with self.assertRaisesRegex(ValueError,"cardinality mismatch"):
                receipts.validate_cache(row,arm,1)

    def test_streamed_status_must_agree_with_pages(self):
        row = streamed()
        row["residency"]["resident_pages"] = 1
        with self.assertRaisesRegex(ValueError,"status and observed residency disagree"):
            receipts.validate_cache(row,"N",0,True)

    def test_zero_page_main_does_not_replace_resident_auxiliary_predicate(self):
        row = scalar()
        row["declared_external_inputs"] = dict(replay_roots=[],semantic_files=["/code/node-roots.json"])
        row["auxiliary_residencies"] = [dict(role="semantic",required_residency_paths=["/code/node-roots.json"],
            residency=dict(cache_class="A",status="INELIGIBLE",resident_pages=1,payload_bytes_read=0,attempts=0,method="fadvise mincore",
                files=[dict(path="/code/node-roots.json",present=True,resident_pages=1,eviction_hint_attempts=1)]))]
        receipts.validate_cache(row,"L",0,True)
        with self.assertRaisesRegex(ValueError,"additional cold predicate"):
            receipts.validate_cache(row,"L",1,False)

    def test_ineligible_envelope_keeps_all_original_attempt_counts_zero(self):
        row = test_registry.diagnostic("E02:A:L")
        row.update(row_status="INELIGIBLE",attempted_operation_count=0,completed_operation_count=0,sample_count=0)
        row["cache"] = scalar()
        row["cache"]["residency"]["files"][1].update(present=True,resident_pages=1,eviction_hint_attempts=1)
        row["cache"]["residency"].update(status="INELIGIBLE",resident_pages=1)
        self.assertEqual(receipts.validate_receipt(row)["row_status"],"INELIGIBLE")
        row["performance_status"] = "PASS"
        with self.assertRaisesRegex(ValueError,"unrun or cache-ineligible"):
            receipts.validate_receipt(row)

    def test_unknown_a_cache_cannot_claim_performance_pass(self):
        row = test_registry.diagnostic("E02:A:N")
        row["cache"] = scalar()
        row["cache"]["residency"]["status"] = "UNAVAILABLE"
        row["performance_status"] = "PASS"
        with self.assertRaisesRegex(ValueError,"unknown or ineligible cold predicate"):
            receipts.validate_receipt(row)

    def test_zero_file_pages_cannot_promote_a_l_post_mount_internal_unknown_to_pass(self):
        row = test_registry.diagnostic("E02:A:L")
        row["cache"] = scalar()
        receipts.validate_receipt(row)
        self.assertEqual(row["row_status"],"INCOMPLETE")
        row["performance_status"] = "PASS"
        with self.assertRaisesRegex(ValueError,"post-Mount internal cold command state is unavailable"):
            receipts.validate_receipt(row)

    def test_unrun_row_cannot_add_a_performance_pass(self):
        row = test_registry.unrun_rows()[0]
        row["performance_status"] = "PASS"
        with self.assertRaisesRegex(ValueError,"unrun or cache-ineligible"):
            receipts.validate_receipt(row)


if __name__ == "__main__":
    unittest.main()
