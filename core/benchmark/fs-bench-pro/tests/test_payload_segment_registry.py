"""Explicit candidate identity and complete durable backing allocation."""
import sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from families import phase7_sqlite as f
from shared import sqlite_contract as c
class PayloadSegments(unittest.TestCase):
    def test_new_candidate_keeps_original_gates_and_reference_identity(self):
        new=f.CASES[f.PAYLOAD_SEGMENT_CASE];old=f.CASES[f.PAYLOAD_SEGMENT_REFERENCE]
        self.assertEqual((new.fixture,new.profile,new.command_budget_ns,new.verification_budget_ns,new.proof_policy,new.proof_envelope),(old.fixture,old.profile,old.command_budget_ns,old.verification_budget_ns,old.proof_policy,old.proof_envelope))
        self.assertEqual(new.pack_layout,'payload-segments');self.assertEqual(old.pack_layout,'monolithic')
        self.assertEqual((new.command_budget_ns,new.verification_budget_ns),(30_000_000_000,19_000_000_000))
        self.assertIn('all-segments',f.PAYLOAD_SEGMENT_ALLOCATION_RULE)
    def test_all_payload_files_and_directory_are_charged_including_orphans(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)/'db';p.write_bytes(b'db');folder=Path(str(p)+'.payload');folder.mkdir()
            (folder/'1.segment').write_bytes(b'a'*5000);(folder/'unacknowledged.segment').write_bytes(b'b'*10000)
            got=c.allocations([p],include_payloads=True)
            expected=[p,folder,folder/'1.segment',folder/'unacknowledged.segment']
            self.assertEqual(got['total_bytes'],sum(x.stat().st_blocks*512 for x in expected));self.assertEqual(len(got['files']),4)
            (folder/'symlink').symlink_to(p)
            with self.assertRaises(ValueError):c.allocations([p],include_payloads=True)
    def test_original_allocation_scope_and_no_missing_directory_credit(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)/'db';p.write_bytes(b'db')
            self.assertEqual(len(c.allocations([p])['files']),1)
            with self.assertRaises(ValueError):c.allocations([p],include_payloads=True)
