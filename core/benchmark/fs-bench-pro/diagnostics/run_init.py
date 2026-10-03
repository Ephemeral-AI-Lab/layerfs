#!/usr/bin/env python3
"""One count-driven cause diagnostic per case/arm; never a speed admission row."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import fcntl

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent));sys.path.insert(0,str(HERE))
import runner
from diagnostics import services as observer
from diagnostics.minio_trace import Trace
from families import init_namespace as init
from families.phase7_storage import invoke
from shared import phase7 as contract
BASE=runner.ROOT/'target/phase7-baseline/layerfs'
CASES={'phase7-init-work-100-v1':'100','phase7-init-work-1000-v1':'1000'}
CASE_IDS={'100':'namespace-100-compact-v3','1000':'namespace-1000-compact-v3'}


def archive(path):
    sha=runner.digest(path);home=runner.RESULTS/'binary-archive'/sha/path.name
    home.parent.mkdir(parents=True,exist_ok=True)
    if not home.exists():shutil.copy2(path,home);home.chmod(0o555)
    return {'path':str(home),'sha256':sha}


def run(case_id,arm,out):
    raise ValueError("PostgreSQL/MinIO cause campaign is retired; retain existing receipts without new service runs")
    if sys.platform != "darwin":raise ValueError("this external SQLite diagnostic is qualified on macOS only")
    out=runner.owned(out);out.mkdir(parents=True)
    with (runner.RESULTS/'phase7.lock').open('a+b') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        identity=runner.identities()
        if identity['source_dirty']:raise ValueError('freeze committed diagnostic source before running')
        with (BASE/'target/phase7.lock').open('a+b') as base_lock:
            fcntl.flock(base_lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            if subprocess.check_output(['git','status','--porcelain'],cwd=BASE,text=True):raise ValueError('baseline checkout must be clean')
            base_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=BASE,text=True).strip()
            if base_head!='7edddbdb8e8512627aed0ed42533ef099d802384':raise ValueError('baseline source pin changed')
            case=init.CASES[CASE_IDS[case_id]]
            fixture=init.prepare(case,runner.RESULTS/'sdk-prepared')
            binary=(BASE/'core/target/release/examples/diagnose_init' if arm=='baseline' else runner.CORE/'target/release/examples/diagnose_init')
            executable=archive(binary)
            verifier=archive(BASE/'core/target/release/examples/verify_namespace' if arm=='baseline' else runner.CORE/'target/release/examples/verify_namespace')
            prepare=archive(runner.CORE/'target/release/examples/prepare_storage')
            dylib=archive(runner.ROOT/'target/phase7-agent/diagnostic-tools/sqlite_work.dylib')
            observation_seal=runner.seal(list(HERE.glob('*.*')))
            identity['root_cargo_config_sha256']=runner.digest(runner.ROOT/'.cargo/config.toml')
            identity['shipped_sql_seal']=runner.seal(list((runner.CORE/'crates').glob('*/sql/**/*.sql')))
            record={'schema':'phase7-init-cause-diagnostic-v1','kind':'CAUSE_DIAGNOSTIC','admission_eligible':False,'arm':arm,'files':case.files,'case':f'phase7-init-work-{case_id}-v1','source':identity,'baseline_source':base_head,'observer_seal':observation_seal,'fixture':fixture,'binary':executable,'verifier':verifier,'sqlite_observer':dylib,'status':'NOT_RUN','sample_count':0,'scope':'public Phase4.5 Service import vs direct project init; equal explicit authority/seed; no SDK speed row','budgets':{'command_ns':15_000_000_000,'verifier_ns':9_500_000_000},'cache_contract':contract.CACHE,'competing_work':runner.competing_work()}
            runner.write_json(out/'receipt.json',record)
            contract.services.down();record['services']=contract.services.up()
            record['fresh']=contract.empty_services()
            record['pg_observer']=observer.configure()
            env={**os.environ,**contract.services.load()['environment'],'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32}
            if arm=='candidate':
                record['setup']=invoke([prepare['path']],out,'schema-setup',30_000_000_000,env,runner.ROOT)
                if record['setup']['exit_code']!=0:raise ValueError('schema setup failed')
            contract.services.postgres_sql('CHECKPOINT; SELECT pg_stat_statements_reset();')
            record['machine']=json.loads(contract.services.docker('info','--format','{{json .}}').stdout)
            record['machine']={k:record['machine'].get(k) for k in ('NCPU','MemTotal','Architecture','OSType','KernelVersion')}
            record['before']=observer.snapshot()
            trace=Trace(out) if arm=='candidate' else None
            record['residency']=contract.dewarm_tree(fixture['source'])
            if record['residency']['status']!='PASS':
                if trace:record['minio_trace']=trace.stop()
                record['status']='INELIGIBLE';runner.write_json(out/'receipt.json',record);return
            scratch=out/'scratch';scratch.mkdir()
            env['TMPDIR']=str(scratch)
            store,history=out/'store.sqlite',out/'history.sqlite'
            if arm=='baseline':
                env.update(DYLD_INSERT_LIBRARIES=dylib['path'],LAYERFS_SQLITE_WORK_OUTPUT=str(out/'sqlite-work.json'),LAYERFS_SQLITE_SCOPE='setup')
                command=[executable['path'],fixture['source'],str(store),str(history),str(out)]
            else:command=[executable['path'],fixture['source'],str(scratch),str(out)]
            claim=runner.RESULTS/'phase7-diagnostic-claims'/hashlib.sha256(f'{case_id}|{arm}|{executable["sha256"]}|{observation_seal}|{fixture["manifest_sha256"]}'.encode()).hexdigest()
            claim.parent.mkdir(parents=True,exist_ok=True)
            with claim.open('x') as file:file.write(str(out)+'\n')
            if trace:trace.start_product()
            try:record['driver']=invoke(command,out,'driver',15_000_000_000,env,runner.ROOT)
            finally:
                if trace:record['minio_trace']=trace.stop()
            record['sample_count']=1
            record['after']=observer.snapshot()
            runner.write_json(out/'pg-statements.json',observer.statements())
            child=record['driver']['child']
            record['status']='COMPLETE' if record['driver']['exit_code']==0 and isinstance(child,dict) and child.get('status')=='PASS' else 'FAIL'
            if record['status']=='COMPLETE':
                # The external SQLite observer is restricted to the diagnostic child.
                proof_env={**env};proof_env.pop('DYLD_INSERT_LIBRARIES',None);proof_env.pop('LAYERFS_SQLITE_WORK_OUTPUT',None)
                proof=[verifier['path'],str(store),str(history),child['root'],child['stack'],fixture['manifest'],fixture['manifest_sha256']]
                record['verification']=invoke(proof,out,'verifier',9_500_000_000,proof_env,runner.ROOT)
                record['verification_status']='PASS' if record['verification']['exit_code']==0 and runner.lite_verification_pass(record['verification']['child'],case,child,fixture) else 'FAIL'
                if arm=='candidate':record['storage']=contract.collect()
                else:
                    import sqlite3
                    with sqlite3.connect(f'file:{store}?mode=ro',uri=True) as db:
                        record['canonical']=dict(zip(('objects','bytes'),db.execute('SELECT COUNT(*),SUM(n) FROM (SELECT object_id,MAX(canonical_length) n FROM objects GROUP BY object_id)').fetchone()))
                    record['storage_bytes']=sum(p.stat().st_blocks*512 for p in (store,history))
            record['scratch_cleanup']='PASS' if not list(scratch.iterdir()) else 'FAIL'
            runner.write_json(out/'receipt.json',record);runner.manifest_run(out)
            print(out)
