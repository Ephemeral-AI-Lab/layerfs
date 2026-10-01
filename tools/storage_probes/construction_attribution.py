"""One bounded count diagnostic; original throughput receipts never changed."""
import argparse
import fcntl
import json
from pathlib import Path
import resource
import shutil
import sqlite3
import sys

from common import write_json
from repository_probe import deadline_command, sha256_file
from run import inventory


def invariants(output, reference):
    output, reference = Path(output), Path(reference)
    db = sqlite3.connect(output/'stage/catalog.sqlite',isolation_level=None,timeout=0)
    db.execute('PRAGMA query_only=ON');db.execute('PRAGMA cache_size=-512')
    db.execute('PRAGMA mmap_size=0');db.execute('PRAGMA temp_store=FILE')
    db.execute('ATTACH DATABASE ? AS reference',(str(reference/'stage/catalog.sqlite'),))
    columns={'entries':'path,kind,mode,mtime,size,dev,ino,uid,gid,flags,link,sha256,content_root',
             'xattrs':'path,name,value', 'objects':'id,role,canonical_length,pack_id,group_no,record_no',
             'packs':'id,size,blake3'}
    for table,cols in columns.items():
        for a,b in [('main','reference'),('reference','main')]:
            count=db.execute(f'SELECT count(*) FROM(SELECT {cols} FROM {a}.{table} EXCEPT SELECT {cols} FROM {b}.{table})').fetchone()[0]
            assert count==0,(table,a,b,count)
    assert db.execute('SELECT ready FROM main.publication').fetchone()==(0,)
    assert db.execute('SELECT ready FROM reference.publication').fetchone()==(1,)
    packs=total=0
    for number,size in db.execute('SELECT id,size FROM main.packs ORDER BY id'):
        name=f'pack-{number:08d}.bin'
        with (output/'stage/packs'/name).open('rb') as actual, (reference/'stage/packs'/name).open('rb') as original:
            read=0
            while True:
                a=actual.read(65536);b=original.read(65536)
                assert a==b,'pack-byte mismatch'
                if not a:break
                read+=len(a)
            assert read==size
        packs+=1;total+=size
    db.close()
    assert (packs,total)==(5836,1389188326)
    write_json(output/'verification.json',{'status':'PASS','pack_bytes_equal':True,
               'catalog_tables_equal':list(columns),'packs':packs,'bytes':total,
               'only_difference':'diagnostic ready=0 versus reference ready=1',
               'oracle':'original independently reconstructed corpus/pack proof, pinned by SHA256'})


def run(args):
    output=Path(args.output).resolve();assert not output.exists();output.mkdir(parents=True)
    source=inventory();assert not source['tracked_dirty'],'freeze source before diagnostic'
    master,reference=Path(args.master).resolve(),Path(args.reference).resolve()
    binary=Path(args.binary).resolve()
    assert sha256_file(master/'manifest.sqlite')=='541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af'
    assert sha256_file(reference/'stage/catalog.sqlite')=='8514f24e74c1718fd1839a60428fa894316cc6c449772c500521bbe2968c150b'
    assert shutil.disk_usage(output).free>=12*1024**3
    identity={'source':source,'binary_sha256':sha256_file(binary),
              'cargo_config_sha256':sha256_file('.cargo/config.toml'),
              'master_manifest_sha256':sha256_file(master/'manifest.sqlite'),
              'reference_catalog_sha256':sha256_file(reference/'stage/catalog.sqlite'),
              'reference_original_proof_sha256':sha256_file(reference/'verification-command.json'),
              'reference_exhaustive_proof_sha256':sha256_file('benchmark-results/storage-probes/deepseek-reconstruction-remainder-v2/aggregate.json'),
              'diagnostic':'D-deepseek-construction-attribution-v1','performance_claim':False}
    write_json(output/'identity.json',identity)
    with (output.parent/'.measurement.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        before=resource.getrusage(resource.RUSAGE_CHILDREN)
        result=deadline_command([str(binary),'diagnose',str(master),str(output/'stage')],output/'diagnostic.log',180)
        after=resource.getrusage(resource.RUSAGE_CHILDREN)
        result.update({'child_user_seconds':after.ru_utime-before.ru_utime,
                       'child_system_seconds':after.ru_stime-before.ru_stime,
                       'cpu_scope':'complete single Rust child, not per-region CPU or lifetime peak',
                       'performance_claim':False})
        write_json(output/'diagnostic-command.json',result)
        print('diagnostic',result['status'],flush=True)
        if result['status']!='COMPLETE':return
        measured=json.loads((output/'stage/attribution.json').read_text())
        assert measured['counts']=={'files':103108,'logical_bytes':3475776149,
               'unique_objects':210332,'duplicates':45212,'packs':5836,'pack_bytes':1389188326}
        proof=deadline_command([sys.executable,str(Path(__file__).resolve()),'verify',
                                '--output',str(output),'--reference',str(reference)],output/'verification.log',10)
        write_json(output/'verification-command.json',proof)
        print('output invariants',proof['status'],flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action',choices=['run','verify'])
    p.add_argument('--output',required=True)
    p.add_argument('--master',default='benchmark-results/storage-probes/deepseek-full-master-v2')
    p.add_argument('--reference',default='benchmark-results/storage-probes/deepseek-full-import-v1')
    p.add_argument('--binary',default='core/target/release/examples/minio_repository_probe')
    args=p.parse_args()
    if args.action=='run':run(args)
    else:invariants(args.output,args.reference)
