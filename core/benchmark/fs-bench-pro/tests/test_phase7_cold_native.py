"""Real resident/cold primitive equivalence and fail-closed input custody."""
import json
import os
import sqlite3
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import cold_native, sqlite_contract
from families.phase7_sqlite import invoke
import runner


@unittest.skipUnless(sys.platform == 'darwin', 'qualified native profile requires macOS')
class ColdNative(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory(dir=runner.ROOT / 'target/phase7-agent')
        cls.out = Path(cls.scratch.name)
        cls.helper = cold_native.build(runner.ROOT, cls.out, invoke)

    @classmethod
    def tearDownClass(cls):
        cls.scratch.cleanup()

    def test_resident_invalidation_and_cold_attestation_match_existing_primitive(self):
        root = self.out / 'ordinary'
        root.mkdir()
        page = os.sysconf('SC_PAGESIZE')
        values = [b'', b'a', b'b' * (page * 3 + 1)]
        for index, value in enumerate(values):
            (root / str(index)).write_bytes(value)
        # Explicit self-test preconditioning on disposable files, never benchmark input.
        for path in root.iterdir():
            path.read_bytes()
        expected = sqlite_contract.dewarm_tree(root)
        self.assertGreater(expected['resident_first'], 0)
        for path in root.iterdir():
            path.read_bytes()
        warm_out = self.out / 'warm'; warm_out.mkdir()
        actual = cold_native.attest(root, self.helper, warm_out, 2_000_000_000, invoke, runner.ROOT)
        for field in ('files', 'length_bytes', 'total_pages', 'resident_after', 'status'):
            self.assertEqual(actual[field], expected[field])
        self.assertGreater(actual['resident_first'], 0)
        self.assertEqual(actual['msync_calls'], 2)
        self.assertEqual(actual['invalidated_files'], 2)
        self.assertEqual(actual['mmap_calls'], 4)
        self.assertEqual(actual['mincore_calls'], 6)
        self.assertEqual(actual['opens'], 8)
        cold_out = self.out / 'cold'; cold_out.mkdir()
        cold = cold_native.attest(root, self.helper, cold_out, 2_000_000_000, invoke, runner.ROOT)
        self.assertEqual(cold['resident_first'], 0)
        self.assertEqual(cold['resident_after'], 0)
        self.assertEqual(cold['msync_calls'], 0)
        self.assertEqual(cold['mmap_calls'], 4)
        self.assertEqual(cold['fstats'], 6)

    def test_symlink_special_entry_empty_inventory_and_non_directory_refuse(self):
        for label, kind in [('symlink', 'symlink'), ('fifo', 'fifo'), ('empty', 'empty'), ('file', 'file')]:
            root = self.out / label
            root.mkdir()
            if kind == 'symlink':
                (root / 'bad').symlink_to(self.out)
            elif kind == 'fifo':
                os.mkfifo(root / 'bad')
            elif kind == 'file':
                root = root / 'bad'; root.write_bytes(b'a')
            out = self.out / (label + '-result'); out.mkdir()
            with self.assertRaisesRegex(ValueError, 'attestation failed'):
                cold_native.attest(root, self.helper, out, 2_000_000_000, invoke, runner.ROOT)
            self.assertTrue((out / 'cold.stderr').read_text())

    @unittest.skipIf(os.geteuid() == 0, 'root bypasses file permission checks')
    def test_readonly_nonresident_source_passes_but_warm_invalidation_refuses(self):
        root = self.out / 'readonly'; root.mkdir()
        path = root / 'data'; path.write_bytes(b'x' * os.sysconf('SC_PAGESIZE'))
        self.assertTrue(sqlite_contract.residency.de_warm(path).dewarmed)
        path.chmod(0o444)
        cold_out = self.out / 'readonly-cold'; cold_out.mkdir()
        cold = cold_native.attest(root, self.helper, cold_out, 2_000_000_000, invoke, runner.ROOT)
        self.assertEqual(cold['resident_after'], 0)
        path.read_bytes()
        warm_out = self.out / 'readonly-warm'; warm_out.mkdir()
        with self.assertRaisesRegex(ValueError, 'attestation failed'):
            cold_native.attest(root, self.helper, warm_out, 2_000_000_000, invoke, runner.ROOT)
        path.chmod(0o644)

    def test_seal_mismatch_refuses_before_invocation(self):
        bad = json.loads(json.dumps(self.helper)); bad['binary_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'seal mismatch'):
            cold_native.attest(self.out, bad, self.out, 2_000_000_000, invoke, runner.ROOT)
        self.assertFalse((self.out / 'cold.stdout').exists())

    def test_live_memory_and_wal_owners_attest_all_body_pages_and_refuse_aliases(self):
        for mode in ('MEMORY', 'WAL'):
            path = self.out / f'live-{mode}.sqlite'
            with sqlite3.connect(path) as db:
                db.execute(f'PRAGMA journal_mode={mode}')
                db.execute('PRAGMA mmap_size=0')
                db.execute('CREATE TABLE values_fixture (body BLOB)')
                db.execute('INSERT INTO values_fixture VALUES (?)', (b'x' * 1048576,))
                db.commit()
                paths = [str(path)]
                if mode == 'WAL': paths.append(str(path) + '-wal')
                result = subprocess.run([self.helper['binary'], '--files', *paths],
                                        capture_output=True, text=True, timeout=2, check=True)
                row = json.loads(result.stdout)
                self.assertEqual(row['files'], len(paths))
                self.assertEqual(row['resident_after'], 0)
                self.assertGreater(row['resident_first'], 0)
                # This capability check must preserve live SQLite contents.
                self.assertEqual(db.execute('SELECT length(body) FROM values_fixture').fetchone(), (1048576,))
                bad = subprocess.run([self.helper['binary'], '--files', str(path), str(path)],
                                     capture_output=True, text=True, timeout=2)
                self.assertNotEqual(bad.returncode, 0)
                self.assertIn('duplicate file identity', bad.stderr)

    def test_mixed_corpus_roots_cover_manifest_oracles_inputs_without_other_data(self):
        corpus = self.out / 'corpus-shape'; corpus.mkdir()
        roots = [corpus / 'manifest.json', corpus / 'inputs', corpus / 'oracles']
        roots[0].write_bytes(b'manifest')
        roots[1].mkdir(); roots[2].mkdir()
        (roots[1] / 'blob').write_bytes(b'x' * 32768)
        (roots[2] / 'oracle').write_bytes(b'oracle')
        # Unrelated history data is intentionally outside the declared inventory.
        (corpus / 'unrelated').write_bytes(b'not measured')
        out = self.out / 'mixed-cold'; out.mkdir()
        row = cold_native.attest_paths(roots, self.helper, out, 2_000_000_000, invoke, runner.ROOT)
        self.assertEqual(row['files'], 3)
        self.assertEqual(row['length_bytes'], 32768 + 8 + 6)
        self.assertEqual(row['resident_after'], 0)
        self.assertEqual(row['status'], 'PASS')
