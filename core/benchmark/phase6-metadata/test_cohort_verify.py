import importlib.util,json,sqlite3,tempfile,unittest
from pathlib import Path
BASE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('cohort_oracle',BASE/'verify_cohort.py')
oracle=importlib.util.module_from_spec(spec);spec.loader.exec_module(oracle)
class CohortOracleTests(unittest.TestCase):
    def fixture(self,folder):
        case='cohort-overwrite-4k'
        nodes,names,sources,dirty,spans,_=oracle.model(case)
        db=sqlite3.connect(folder/'sample.sqlite');db.executescript((BASE/'schema.sql').read_text()+(BASE/'cohort.sql').read_text())
        db.execute('INSERT INTO workspace(w,active) VALUES(1,2)')
        for ino,(value,size,kind) in nodes.items():
            db.execute('INSERT INTO kinds VALUES(?,?)',(ino,kind))
            db.execute('INSERT INTO inodes VALUES(1,?,1,?,?,?)',(ino,oracle.LIVE,value,size))
        for (parent,name),ino in names.items():db.execute('INSERT INTO names VALUES(1,?,?,1,?,?)',(parent,name,oracle.LIVE,ino))
        for id,(fill,data,length) in sources.items():db.execute('INSERT INTO sources VALUES(?,?,?,?)',(id,fill,data,length))
        for start,end,id,off in spans:db.execute('INSERT INTO extents VALUES(1,1,?,1,?,?,?,?)',(start,oracle.LIVE,end,id,off))
        db.commit();db.close()
        (folder/'final.catalog').write_text('\n'.join(oracle.catalog(nodes,names,dirty,spans,1))+'\n')
        (folder/'operation.json').write_text(json.dumps({'dirty_inodes':1,'obsolete_versions':0}))
    def test_wrong_payload_detected(self):
        with tempfile.TemporaryDirectory() as t:
            folder=Path(t);self.fixture(folder)
            self.assertEqual(oracle.verify('cohort-overwrite-4k',folder)['status'],'PASS')
            db=sqlite3.connect(folder/'sample.sqlite');db.execute('UPDATE sources SET data=? WHERE id=20000',(b'Q'*4096,));db.commit();db.close()
            with self.assertRaises(AssertionError):oracle.verify('cohort-overwrite-4k',folder)
    def test_dirty_owner_leak_detected(self):
        with tempfile.TemporaryDirectory() as t:
            folder=Path(t);self.fixture(folder)
            db=sqlite3.connect(folder/'sample.sqlite');db.execute('INSERT INTO changed VALUES(1,1,1)');db.commit();db.close()
            with self.assertRaises(AssertionError):oracle.verify('cohort-overwrite-4k',folder)
    def test_extra_result_row_detected(self):
        with tempfile.TemporaryDirectory() as t:
            folder=Path(t);self.fixture(folder)
            with (folder/'final.catalog').open('a') as f:f.write('extra\n')
            with self.assertRaises(AssertionError):oracle.verify('cohort-overwrite-4k',folder)
if __name__=='__main__':unittest.main()
