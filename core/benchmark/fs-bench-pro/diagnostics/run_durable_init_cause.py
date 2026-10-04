#!/usr/bin/env python3
"""One declared count-driven Durable10k cause run; not a speed qualification."""
import argparse,fcntl,json,os,re,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke
from shared import cold_native,history_observer

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--out',required=True);parser.add_argument('--source-receipt',required=True);args=parser.parse_args()
    identity=runner.identities()
    if identity['source_dirty']:raise ValueError('freeze diagnostic source/harness first')
    prior=json.loads(Path(args.source_receipt).read_text());driver=prior['build']['binaries']['benchmark_init'];fixture=prior['fixture']
    if prior['requested_profile']!='durable' or fixture['files']!=10000 or prior['verification_status']!='PASS' or runner.digest(driver['path'])!=driver['sha256']:raise ValueError('qualified release driver/fixture identity required')
    out=runner.owned(args.out);out.mkdir();lock=(runner.RESULTS/'phase7-sqlite.lock').open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    record={'kind':'durable-init10k-write-cause-v1','admission':'NOT_APPLICABLE','verification':'SKIPPED; prior qualified proof and root cross-check, diagnostic only','identity':identity,'prior_receipt':str(Path(args.source_receipt).resolve()),'prior_receipt_sha256':runner.digest(args.source_receipt),'driver':driver,'fixture':fixture,'sample_count':0,'status':'INCOMPLETE','budget_ns':30_000_000_000,'orchestrator_sha256':runner.digest(Path(__file__))}
    try:
        observer=history_observer.build(ROOT,out,invoke,runner);record['observer']=observer
        helper=cold_native.build(ROOT,out,invoke);scratch=out/'scratch';scratch.mkdir();db=out/'store.sqlite'
        start=time.monotonic_ns();record['residency']=cold_native.attest(fixture['source'],helper,out,record['budget_ns'],invoke,ROOT)
        if record['residency']['status']!='PASS':record['status']='INELIGIBLE';return
        env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch),'DYLD_INSERT_LIBRARIES':observer['path'],'LAYERFS_SQLITE_WORK_OUTPUT':str(out/'sql-work.json'),'LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json'),'LAYERFS_CLOSE_OBSERVER_OUTPUT':str(out/'close.json')}
        command=[driver['path'],fixture['source'],str(db),str(scratch),'durable-init10k-write-cause-v1','durable','30'];record['run']=invoke(command,out,'driver',record['budget_ns']-(time.monotonic_ns()-start),env,ROOT);record['sample_count']=1;record['command_wall_ns']=time.monotonic_ns()-start
        run=record['run']
        if run['exit_code'] or run['timed_out']:record['status']='FAIL';return
        if run['child']['root']!=prior['performance']['child']['root']:raise ValueError('original canonical root differs')
        observations=history_observer.collect(out);vfs=observations['vfs']
        for kind in vfs['files']:
            scoped=[r for r in vfs['call_scopes'] if r['class']==kind['class']]
            for key in ('writes','write_submitted_bytes','write_ns','syncs','sync_ns'):
                if sum(r[key] for r in scoped)!=kind[key]:raise ValueError('scoped VFS accounting mismatch')
        if any(r['errors'] for r in vfs['call_scopes']):raise ValueError('observed I/O errors')
        record.update(status='DIAGNOSTIC',root_match=True,observations=observations,cleanup='PASS' if not list(scratch.iterdir()) and not list(out.glob('.layerfs-allocation-*')) else 'FAIL')
        print(json.dumps({'status':record['status'],'command_wall_ns':record['command_wall_ns'],'vfs':vfs}))
    finally:
        runner.write_json(out/'receipt.json',record);runner.manifest_run(out);lock.close()
if __name__=='__main__':main()
