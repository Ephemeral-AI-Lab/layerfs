"""Prospective construction mechanism treatments; one attempt per arm."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import sqlite3
import sys
import time

from common import write_json
from repository_probe import deadline_command, sha256_file
from run import inventory


def setup(args):
    root=Path(args.fixtures).resolve();root.mkdir(parents=True,exist_ok=False)
    assert shutil.disk_usage(root).free>=8*1024**3
    for mode in ['stream','sized']:(root/mode).mkdir()
    ref=Path(args.reference).resolve();master=Path(args.master).resolve()
    db=sqlite3.connect('file:'+str(ref/'stage/catalog.sqlite')+'?mode=ro',uri=True)
    db.execute('PRAGMA cache_size=-512');db.execute('PRAGMA mmap_size=0')
    started=time.monotonic_ns();count=total=0
    for raw,size,digest in db.execute("SELECT path,size,sha256 FROM entries WHERE kind='file' AND size>0 AND size<131072 ORDER BY path"):
        rel=Path(os.fsdecode(raw));destinations=[root/mode/rel for mode in ['stream','sized']]
        for dest in destinations:dest.parent.mkdir(parents=True,exist_ok=True)
        hashed=hashlib.sha256();read=0
        with (master/'tree'/rel).open('rb') as source, destinations[0].open('xb') as a, destinations[1].open('xb') as b:
            while chunk:=source.read(65536):
                hashed.update(chunk);read+=len(chunk);a.write(chunk);b.write(chunk)
        assert read==size and hashed.hexdigest()==digest
        count+=1;total+=read
        if count%8192==0:print('fixture files',count,flush=True)
    db.close();assert count==101494 and total==523127919
    write_json(root/'seal.json',{'files':count,'bytes_per_arm':total,'wall_ns':time.monotonic_ns()-started,
                'copy':'independent ordinary byte copies; no APFS clone/source hardlinks; two trees acquired together once',
                'validation':'every SHA256/size matched independent captured manifest',
                'reference_catalog_sha256':sha256_file(ref/'stage/catalog.sqlite'),
                'source_data_preconditioning':'setup writes may leave pages resident; cache equality UNKNOWN; not cold'})


def verify(output,reference):
    output,reference=Path(output),Path(reference)
    db=sqlite3.connect('file:'+str(output/'catalog.sqlite')+'?mode=ro',uri=True)
    db.execute('PRAGMA cache_size=-512');db.execute('PRAGMA mmap_size=0');db.execute('PRAGMA temp_store=FILE')
    db.execute('ATTACH DATABASE ? AS reference',(str(reference/'stage/catalog.sqlite'),))
    for table,cols in [('entries','path,kind,mode,mtime,size,dev,ino,uid,gid,flags,link,sha256,content_root'),('xattrs','path,name,value')]:
        for a,b in [('main','reference'),('reference','main')]:
            assert db.execute(f'SELECT count(*) FROM(SELECT {cols} FROM {a}.{table} EXCEPT SELECT {cols} FROM {b}.{table})').fetchone()==(0,)
    assert db.execute("SELECT count(*) FROM entries WHERE kind='file' AND content_root IS NOT NULL").fetchone()==(103108,)
    db.close();write_json(output/'verification.json',{'status':'PASS','all_file_roots':103108,'all_metadata_equal':True})


def run(args):
    output=Path(args.output).resolve();output.mkdir(parents=True,exist_ok=False)
    source=inventory();assert not source['tracked_dirty']
    fixtures=Path(args.fixtures).resolve();seal=json.loads((fixtures/'seal.json').read_text())
    ref=Path(args.reference).resolve();master=Path(args.master).resolve();binary=Path(args.binary).resolve()
    assert sha256_file(ref/'stage/catalog.sqlite')==seal['reference_catalog_sha256']=='8514f24e74c1718fd1839a60428fa894316cc6c449772c500521bbe2968c150b'
    assert sha256_file(master/'manifest.sqlite')=='541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af'
    write_json(output/'identity.json',{'source':source,'binary_sha256':sha256_file(binary),
               'cargo_config_sha256':sha256_file('.cargo/config.toml'),'fixture_seal':seal,
               'performance_claim':False,'cache_verdict':'INELIGIBLE'})
    with (output.parent/'.measurement.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        cases=[('root','uncached',15),('root','cached',15),('small','stream',25),('small','sized',25)]
        for mechanism,mode,limit in cases:
            name=f'O-{mechanism}-{mode}-v1';case=output/name
            log=output/(name+'.log');assert not log.exists() and not case.exists()
            if mechanism=='root':argv=[str(binary),'opt-root',str(master),str(ref/'stage/catalog.sqlite'),str(case),mode]
            else:argv=[str(binary),'opt-small',str(ref/'stage/catalog.sqlite'),str(fixtures/mode),str(case),mode]
            before=resource.getrusage(resource.RUSAGE_CHILDREN)
            receipt=deadline_command(argv,log,limit);after=resource.getrusage(resource.RUSAGE_CHILDREN)
            receipt.update({'child_user_seconds':after.ru_utime-before.ru_utime,'child_system_seconds':after.ru_stime-before.ru_stime,'performance_claim':False,'cache_verdict':'INELIGIBLE'})
            case.mkdir(exist_ok=True);write_json(case/'command.json',receipt)
            print(name,receipt['status'],f"{receipt['wall_ns']/1e9:.3f}s",flush=True)
            if receipt['status']!='COMPLETE':
                write_json(case/'verification-command.json',{'status':'NOT_RUN','reason':'arm failed/timed out'});continue
            if mechanism=='root':
                check=deadline_command([sys.executable,str(Path(__file__).resolve()),'verify','--output',str(case),'--reference',str(ref)],case/'verify.log',10)
                write_json(case/'verification-command.json',check)
            else:
                measured=json.loads((case/'result.json').read_text())
                assert measured['files']==101494 and measured['logical_bytes']==523127919
                assert measured['read']['bytes']==523127919 and measured['root_equality']=='PASS'
                write_json(case/'verification.json',{'status':'PASS','all_roots_equal':101494,'source_bytes_read':523127919,'exact_eof':True,'oracle':'original independent byte/reconstruction proofs; acquisition SHA256 verified'})


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action',choices=['setup','run','verify'])
    p.add_argument('--output');p.add_argument('--fixtures',default='benchmark-results/storage-probes/optimization-fixtures-v1')
    p.add_argument('--master',default='benchmark-results/storage-probes/deepseek-full-master-v2')
    p.add_argument('--reference',default='benchmark-results/storage-probes/deepseek-full-import-v1')
    p.add_argument('--binary',default='core/target/release/examples/minio_repository_probe')
    args=p.parse_args()
    if args.action=='setup':setup(args)
    elif args.action=='run':run(args)
    else:verify(args.output,args.reference)
