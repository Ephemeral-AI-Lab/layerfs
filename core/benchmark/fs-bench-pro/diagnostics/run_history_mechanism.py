#!/usr/bin/env python3
"""One labelled17-state mechanism child per arm; never speed admission."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
import subprocess
import time
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families import history_retention as history
from families.phase7_history import build,boundaries
from families.phase7_sqlite import invoke,BASE
from shared import cold_native,history_observer
FIELDS=['statements','vm_steps','step_ns','exec_ns','fullscan_steps','sorts','reprepare','prepare_calls','prepare_ns','reset_calls','reset_ns','vfs_read_requested_bytes','vfs_write_submitted_bytes','vfs_sync_calls','vfs_close_calls','pack_body_acquisitions','new_distinct_pack_ids_lifetime','unattributed_pack_ids','read_begin_calls']


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--out',required=True);args=parser.parse_args()
    identity=runner.identities()
    if identity['source_dirty']:raise ValueError('freeze product and diagnostic harness')
    out=runner.owned(args.out);out.mkdir(parents=True)
    runner.write_json(out/'declaration.json',{'kind':'history17-stage-mechanism-v1','identity':identity,'orchestrator_sha256':runner.digest(Path(__file__)),'samples_per_arm':1,'performance_budget_ns':60_000_000_000,'verification':'SKIPPED; existing canonical expected-root cross-check; diagnostic only','admission':'NOT_APPLICABLE','processing_objective':'parity; <=19.3213359625s supplementary contextual target, whole-operation gate unchanged','fields':FIELDS,'distinct_scope':'new pack IDs first observed during this lifetime, not per-stage unique set; do not call all repeated acquisitions redundant'})
    expected=json.loads((ROOT/'benchmark-results/fs-bench-pro/issue302-history17-v2-baseline1/receipt.json').read_text())['performance']['child']['roots']
    base=ROOT/'target/phase7-baseline/layerfs';locks=[]
    for p in [runner.RESULTS/'phase7-sqlite.lock',base/'target/phase7-sqlite.lock']:
        h=p.open('a+b');fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(h)
    try:
        if subprocess.check_output(['git','rev-parse','HEAD'],cwd=base,text=True).strip()!=BASE or subprocess.check_output(['git','status','--porcelain'],cwd=base,text=True):raise ValueError('pinned clean reference mismatch')
        results={}
        for arm,root in [('baseline',base),('candidate',ROOT)]:
            folder=out/arm;folder.mkdir();vehicle={};b=build(root,arm,folder,runner,vehicle)
            if b['status']!='PASS':raise ValueError('needed build failed')
            if arm=='baseline' and subprocess.check_output(['git','status','--porcelain'],cwd=base,text=True):raise ValueError('reference product changed')
            helper=cold_native.build(ROOT,folder,invoke);observer=history_observer.build(ROOT,folder,invoke,runner)
            corpus=history.corpus.DEFAULT_ROOT;fixture=history.corpus.identity(corpus)
            scratch=folder/'scratch';scratch.mkdir();db=folder/'store.sqlite'
            start=time.monotonic_ns();cold=cold_native.attest_paths([corpus/'checkpoint-manifest.json',corpus/'inputs',corpus/'oracles'],helper,folder,60_000_000_000,invoke,ROOT)
            if cold['status']!='PASS':raise ValueError('diagnostic source cold ineligible')
            env={**os.environ,**history.ENV,'LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch),'LAYERFS_HISTORY_COLD_HELPER':helper['binary'],'DYLD_INSERT_LIBRARIES':observer['path'],'LAYERFS_SQLITE_WORK_OUTPUT':str(folder/'sql-work.json'),'LAYERFS_CAUSE_VFS_LOG':str(folder/'vfs.json'),'LAYERFS_CLOSE_OBSERVER_OUTPUT':str(folder/'close.json')}
            driver=b['binaries']['benchmark_history' if arm=='candidate' else 'history_reference']['path']
            print('START',arm,flush=True)
            result=invoke([driver,str(corpus),str(db),str(scratch/'ordering'),'history-stride10','complete','disposable'],folder,'driver',60_000_000_000-(time.monotonic_ns()-start),env,root)
            record={'kind':'history17-stage-mechanism-v1','status':'DIAGNOSTIC','admission':'NOT_APPLICABLE','sample_count':1,'arm':arm,'identity':identity,'build':b,'vehicle':vehicle,'observer':observer,'cold_helper':helper,'fixture':fixture,'source_cold':cold,'run':result,'command_wall_ns':time.monotonic_ns()-start,'verification':'SKIPPED'}
            if result['exit_code'] or result['timed_out'] or not isinstance(result['child'],dict):record['status']='FAIL'
            else:
                if result['child']['roots']!=expected:raise ValueError('original root vector mismatch')
                text=(folder/'driver.stderr').read_text();record['database_cold']=boundaries(text,17,arm);record['observations']=history_observer.collect(folder)
                entries=[json.loads(s.removeprefix('HISTORY_ENGINE_WORK ')) for s in text.splitlines() if s.startswith('HISTORY_ENGINE_WORK ')]
                if len(entries)!=68 or any(not s['available'] for s in entries):raise ValueError('state observer snapshot missing')
                for entry in entries:entry['work']=dict(zip(FIELDS,entry.pop('values')))
                record['state_engine']=entries;record['provider_logs']=[s for s in text.splitlines() if s.startswith('HISTORY_PROVIDER_WORK ')]
                record['processing_sum_ns']=sum(result['child']['stages_ns'][1:])
                record['cleanup']='PASS' if not list((scratch/'ordering').iterdir()) and not list(folder.glob('.layerfs-allocation-*')) else 'FAIL'
            runner.write_json(folder/'receipt.json',record);runner.manifest_run(folder);results[arm]=record
            print('END',arm,json.dumps({'status':record['status'],'processing_sum_ns':record.get('processing_sum_ns'),'operation_ns':result['child'].get('operation_ns') if result['child'] else None}),flush=True)
        runner.write_json(out/'comparison.json',{'kind':'history17-stage-mechanism-v1','admission':'NOT_APPLICABLE','fields':FIELDS,'processing_ns':{a:r.get('processing_sum_ns') for a,r in results.items()}})
    finally:
        runner.manifest_run(out)
        for h in reversed(locks):h.close()


if __name__=='__main__':main()
