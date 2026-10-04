#!/usr/bin/env python3
"""One fresh history17 count diagnostic; never speed or release admission."""
import fcntl,json,os,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_history import build,boundaries
from families.phase7_sqlite import invoke
from families import history_retention as history
from shared import cold_native,history_observer,sqlite_contract


def phase_counts(stderr,states):
    result={}
    for line in stderr.splitlines():
        if not line.startswith('HISTORY_ENGINE_WORK '):continue
        row=json.loads(line.removeprefix('HISTORY_ENGINE_WORK '))
        if not row['available'] or len(row['values'])!=19:raise ValueError('phase counter unavailable')
        result.setdefault(row['stage'],[]).append(row)
    expected={'acquisition','construction','filesystem','save_custody'}
    if set(result)!=expected:raise ValueError('phase set mismatch')
    for rows in result.values():
        if [r['state'] for r in rows]!=list(range(1,states+1)):raise ValueError('phase state coverage mismatch')
    return {stage:{'states':states,'vfs_requested_bytes':sum(r['values'][11] for r in rows),
                   'pack_acquisitions':sum(r['values'][15] for r in rows)} for stage,rows in result.items()}


def main():
    import argparse
    parser=argparse.ArgumentParser();parser.add_argument('--out',required=True);parser.add_argument('--pack-layout',choices=['monolithic','group-rows'],default='monolithic');args=parser.parse_args()
    (ROOT/'benchmark_agent_report.md').read_text()
    identity=runner.identities()
    if identity['source_dirty']:raise ValueError('freeze count diagnostic source/harness')
    out=runner.owned(args.out);out.mkdir()
    record={'pack_layout':args.pack_layout,'schema':'history17-save-acquisition-cause-v1','status':'NOT_RUN','admission':'NOT_APPLICABLE',
            'identity':identity,'children':1,'performance_samples':0,'complete_budget_ns':60000000000,
            'construction_workers':1,'scope':'counts from all real17states, fresh database, inclusive cold boundaries; no speed gate/verification/admission',
            'comparison':'disjoint phase deltas, never cumulative provider snapshots or device-byte claims',
            'limits':'existing cache/buffer/chain/transaction/visibility/private bounds unchanged',
            'competing_work':runner.competing_work()}
    lock=(runner.RESULTS/'phase7-sqlite.lock').open('a+b')
    try:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        corpus=Path(history.corpus.DEFAULT_ROOT);record['fixture']=history.corpus.identity(corpus)
        record['build']=build(ROOT,'candidate',out,runner,{})
        if record['build']['status']!='PASS':raise ValueError('release build failed')
        observer=history_observer.build(ROOT,out,invoke,runner);helper=cold_native.build(ROOT,out,invoke)
        record.update(observer=observer,cold_helper=helper,setup='reuse sealed release build/prepared source corpus; new database construction remains in child')
        scratch=out/'scratch';scratch.mkdir();db=out/'store.sqlite'
        env={**os.environ,**history.ENV,'TMPDIR':str(scratch),'LAYERFS_HISTORY_CURSOR_KEY':'28'*32,
             'LAYERFS_HISTORY_COLD_HELPER':helper['binary'],'DYLD_INSERT_LIBRARIES':observer['path'],
             'LAYERFS_SQLITE_WORK_OUTPUT':str(out/'sql-work.json'),'LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json'),
             'LAYERFS_CLOSE_OBSERVER_OUTPUT':str(out/'close.json'),'LAYERFS_ACQUISITION_TRACE':str(out/'acquisitions.jsonl')}
        start=time.monotonic_ns();cold=cold_native.attest_paths([corpus/'checkpoint-manifest.json',corpus/'inputs',corpus/'oracles'],helper,out,60000000000,invoke,ROOT)
        record['cold']=cold
        if cold['status']!='PASS':raise ValueError('cold source ineligible')
        remaining=60000000000-(time.monotonic_ns()-start)
        if remaining<=0:raise ValueError('cold preparation exhausted diagnostic command envelope')
        binary=record['build']['binaries']['benchmark_history']['path']
        record['child']=invoke([binary,str(corpus),str(db),str(scratch/'ordering'),'history-stride10','complete','disposable',args.pack_layout],out,'driver',remaining,env,ROOT)
        record['complete_wall_ns']=time.monotonic_ns()-start
        child=record['child'];stderr=(out/'driver.stderr').read_text()
        if child['exit_code'] or child['timed_out'] or not child['child'] or child['child'].get('status')!='COMPLETE':raise ValueError('diagnostic producer incomplete')
        record['phases']=phase_counts(stderr,17);record['database_cold']=boundaries(stderr,17,'candidate')
        record['observations']=history_observer.collect(out);record['allocation']=sqlite_contract.allocations([db])
        ordering=scratch/'ordering'
        if ordering.exists() and list(ordering.iterdir()):raise ValueError('ordering scratch remains')
        record['status']='DIAGNOSTIC_COMPLETE';record['verification']='SKIPPED: mechanism counts only; final independent proof separate'
    except Exception as error:
        record['status']='FAIL';record['reason']=str(error);raise
    finally:
        runner.write_json(out/'receipt.json',record);runner.manifest_run(out);lock.close()
    print(json.dumps(record['phases'],sort_keys=True))


if __name__=='__main__':main()
