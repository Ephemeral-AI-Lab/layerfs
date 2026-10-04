"""Inactive proof-policy proposal: fail-closed zero-frame sidecar checks."""
import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('proposal', Path(__file__).with_name('proof_prototype.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)

class ClosedSidecars(unittest.TestCase):
    def fixture(self, root):
        main = root/'db.sqlite'
        db = sqlite3.connect(main)
        db.execute('PRAGMA journal_mode=WAL')
        db.execute('CREATE TABLE t(a)')
        db.execute('INSERT INTO t VALUES(1)')
        db.commit()
        db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
        db.close()
        Path(str(main)+'-wal').write_bytes(b'')
        Path(str(main)+'-shm').write_bytes(bytes(32768))
        return main

    def test_zero_frame_pair_is_hash_custodied_and_allocations_retained(self):
        with tempfile.TemporaryDirectory() as tmp:
            main = self.fixture(Path(tmp))
            records = p.closed_sidecars(main)
            self.assertEqual([r['length_bytes'] for r in records], [0,32768])
            record = {'path':str(main),'sha256_before':p.digest(main),'closed_sidecars':records}
            p.unchanged_owner(record)
            Path(str(main)+'-shm').write_bytes(b'x'+bytes(32767))
            with self.assertRaisesRegex(ValueError,'changed original sidecars'):
                p.unchanged_owner(record)

    def test_nonempty_wal_and_journal_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            main = self.fixture(Path(tmp))
            Path(str(main)+'-wal').write_bytes(b'frame')
            with self.assertRaisesRegex(ValueError,'frames'):
                p.closed_sidecars(main)
            Path(str(main)+'-wal').write_bytes(b'')
            Path(str(main)+'-journal').touch()
            with self.assertRaisesRegex(ValueError,'journal'):
                p.closed_sidecars(main)

    def test_incomplete_pair_invalid_shm_and_aliases_are_refused(self):
        import os
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); main = self.fixture(root)
            wal = Path(str(main)+'-wal'); shm = Path(str(main)+'-shm')
            shm.unlink()
            with self.assertRaisesRegex(ValueError,'incomplete'):
                p.closed_sidecars(main)
            shm.write_bytes(b'bad')
            with self.assertRaisesRegex(ValueError,'nonexclusive'):
                p.closed_sidecars(main)
            shm.write_bytes(bytes(32768))
            os.link(wal,root/'alias')
            with self.assertRaisesRegex(ValueError,'nonexclusive'):
                p.closed_sidecars(main)
            (root/'alias').unlink(); wal.unlink(); wal.symlink_to(shm)
            with self.assertRaisesRegex(ValueError,'nonexclusive'):
                p.closed_sidecars(main)

    def test_wrong_main_header_and_main_mutation_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            main = self.fixture(Path(tmp)); old = main.read_bytes()
            main.write_bytes(old[:18]+b'\x01\x01'+old[20:])
            with self.assertRaisesRegex(ValueError,'WAL database header'):
                p.closed_sidecars(main)
            main.write_bytes(old)
            record = {'path':str(main),'sha256_before':p.digest(main),'closed_sidecars':p.closed_sidecars(main)}
            main.write_bytes(old+b'x')
            with self.assertRaisesRegex(ValueError,'changed original owner'):
                p.unchanged_owner(record)

    def test_source_growth_cannot_write_beyond_copy_ceiling(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); main = root/'source'; target = root/'copy'
            main.write_bytes(bytes(65536))
            original_open = Path.open
            class Growing:
                def __init__(self, file):
                    self.file = file
                    self.grown = False
                def __enter__(self):
                    return self
                def __exit__(self, *args):
                    self.file.close()
                def fileno(self):
                    return self.file.fileno()
                def read(self, count):
                    block = self.file.read(count)
                    if not self.grown:
                        self.grown = True
                        with original_open(main, 'ab') as writer:
                            writer.write(b'x')
                    return block
            def opened(path, mode='r', *args, **kwargs):
                file = original_open(path, mode, *args, **kwargs)
                return Growing(file) if path == main and mode == 'rb' else file
            with patch.object(Path, 'open', opened):
                with self.assertRaisesRegex(ValueError, 'grew beyond'):
                    p.proof_copy(main, target, 65536)
            self.assertLessEqual(target.stat().st_size, 65536)

    def test_copy_is_independent_and_exact(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); main = self.fixture(root); target=root/'copy.sqlite'
            result = p.proof_copy(main,target,main.stat().st_size)
            self.assertEqual(result['sha256'],p.digest(main))
            self.assertNotEqual(main.stat().st_ino,target.stat().st_ino)
            self.assertEqual(result['buffer_bytes'],65536)
            with self.assertRaisesRegex(ValueError,'ceiling'):
                p.proof_copy(main,root/'too-small.sqlite',main.stat().st_size-1)

if __name__ == '__main__':
    unittest.main()
