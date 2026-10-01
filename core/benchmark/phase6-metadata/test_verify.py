import importlib.util, sqlite3, tempfile, unittest
from pathlib import Path
BASE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('oracle',BASE/'verify.py')
oracle=importlib.util.module_from_spec(spec);spec.loader.exec_module(oracle)

class IndependentOracleTests(unittest.TestCase):
    def fixture(self,folder,size):
        db=sqlite3.connect(folder/'sample.sqlite')
        db.executescript((BASE/'schema.sql').read_text())
        db.execute('INSERT INTO workspace(w,active) VALUES(1,1)')
        db.execute('INSERT INTO inodes VALUES(1,1,1,?,0,?)',(oracle.LIVE,size))
        return db
    def test_repeated_detects_source_coordinate_corruption(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder=Path(tmp);db=self.fixture(folder,16384)
            for start,end,source,off in [(0,4096,0,0),(4096,4128,4097,0),(4128,16384,0,4128)]:
                db.execute('INSERT INTO extents VALUES(1,1,?,1,?,?,?,?)',(start,oracle.LIVE,end,source,off))
            db.commit();db.close()
            self.assertEqual(oracle.verify('extent-repeated-4097',folder)['status'],'PASS')
            db=sqlite3.connect(folder/'sample.sqlite');db.execute('UPDATE extents SET source_offset=source_offset+1 WHERE start=4128');db.commit();db.close()
            with self.assertRaises(AssertionError):oracle.verify('extent-repeated-4097',folder)
    def test_regrow_detects_resurrection(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder=Path(tmp);db=self.fixture(folder,65536)
            for i in range(512):
                db.execute('INSERT INTO extents VALUES(1,1,?,1,?,?,?,0)',(i*8,oracle.LIVE,i*8+8,i%2+1))
            db.execute('INSERT INTO extents VALUES(1,1,4096,1,?,65536,-1,0)',(oracle.LIVE,))
            db.commit();db.close()
            self.assertEqual(oracle.verify('extent-truncate-regrow',folder)['status'],'PASS')
            db=sqlite3.connect(folder/'sample.sqlite');db.execute('UPDATE extents SET source=0 WHERE start=4096');db.commit();db.close()
            with self.assertRaises(AssertionError):oracle.verify('extent-truncate-regrow',folder)
if __name__=='__main__':unittest.main()
