"""External correctness fixture; no throughput sample or product test hooks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import sqlite3
import subprocess

from common import write_json
from minio_probe import start, stop
from repository_probe import perf, verify


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', required=True)
    args = parser.parse_args()
    root = Path(args.root).resolve(); root.mkdir(exist_ok=False)
    master = root / 'master'; (master / 'tree').mkdir(parents=True)
    db = sqlite3.connect(master / 'manifest.sqlite')
    db.executescript('CREATE TABLE entries(path BLOB PRIMARY KEY,kind TEXT,mode INTEGER,mtime INTEGER,size INTEGER,dev INTEGER,ino INTEGER,uid INTEGER,gid INTEGER,flags INTEGER,link BLOB,sha256 TEXT); CREATE TABLE xattrs(path BLOB,name BLOB,value BLOB,PRIMARY KEY(path,name));')
    values = [b'', b'full-source-small\n' * 300, random.Random(291).randbytes(300000)]
    values.append(values[1])
    for i, data in enumerate(values):
        name = f'file-{i}'; (master / 'tree' / name).write_bytes(data)
        db.execute('INSERT INTO entries VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',
                   (name.encode(), 'file', 0o600, 0, len(data), 1, i, 0, 0, 0, None,
                    hashlib.sha256(data).hexdigest()))
    db.execute('INSERT INTO entries VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',
               (b'link','symlink',0o777,0,6,1,5,0,0,0,b'file-1',None))
    db.execute('INSERT INTO xattrs VALUES(?,?,?)',(b'file-1',b'test',b'opaque\x00value'))
    db.commit(); db.close()
    binary = Path('core/target/release/examples/minio_repository_probe').resolve()
    stage = root / 'stage'
    subprocess.run([str(binary), 'prepare', str(master), str(stage)], check=True)
    provider = root / 'minio'; provider.mkdir()
    shutil.copy2('benchmark-results/storage-probes/provider-complete-v1/minio',provider / 'minio')
    start(provider)
    shape = argparse.Namespace(output=str(root),stage=str(stage),minio=str(provider),
             sqlite_provider=str(Path('benchmark-results/storage-probes/sqlite-3.51.3-provider').resolve()))
    try:
        perf(shape)
        # Separate process for public reconstruction; proof remains finite.
        from repository_probe import transfer
        transfer(shape,download=True)
        subprocess.run([str(binary),'verify',str(master),str(stage),str(root/'downloaded-packs')],check=True,timeout=10)
        summary = json.loads((stage/'preparation.json').read_text())
        assert summary['duplicate_emissions'] > 0
        proof = json.loads((stage/'reconstruction.json').read_text())
        assert proof['files'] == 4 and proof['bytes'] == sum(map(len,values))
        write_json(root/'selftest.json',{'status':'PASS','coverage':['empty','small','chunked','dedup','raw xattr','symlink target','actual MinIO body','catalog READY','download decode exact source EOF'], 'performance_sample':False})
    finally:
        stop(provider)


if __name__ == '__main__': main()
