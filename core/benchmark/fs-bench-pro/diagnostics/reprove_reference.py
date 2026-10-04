"""Owner-versioned independent proof; reference performance remains shared evidence."""
import fcntl,json,os,sys
from pathlib import Path
import runner
from families.phase7_sqlite import CASES,invoke
from families.phase7_history import build
from families import history_retention as history
from shared import phase7_history_proof as proof

def reference_run_dir(run):
    supplied=Path(run)
    prior=supplied.resolve()
    if supplied.is_symlink() or not prior.is_relative_to(runner.RESULTS.resolve()) or not prior.is_dir():raise ValueError('retained reference must be an owned existing run directory')
    return prior

def reprove(run,case_id,output):
    case=CASES[case_id]
    if case.states!=157 or case.proof_envelope not in ('owner-stride1-proof15-v2','owner-stride1-proof30-v3'):raise ValueError('explicit owner stride1 proof15 case required')
    identity=runner.identities()
    if identity['source_dirty']:raise ValueError('freeze the owner proof15 harness')
    prior=reference_run_dir(run)
    manifest=json.loads((prior/'manifest.json').read_text())
    for name in ('receipt.json','proof-request.json'):
        seal=manifest['files'][name]
        if (prior/name).stat().st_size!=seal['bytes'] or runner.digest(prior/name)!=seal['sha256']:raise ValueError('retained reference evidence manifest mismatch')
    original=json.loads((prior/'receipt.json').read_text())
    if original['arm']!='baseline' or original['status']!='COMPLETE' or original['workload_row']!='history-stride1' or original['cache_status']!='PASS' or original['cleanup']['status']!='PASS':raise ValueError('eligible original reference performance required')
    for key in ('product_seal','cargo_lock_sha256'):
        if original['identity'][key]!=identity[key]:raise ValueError('changed product/dependency cannot share performance')
    out=runner.owned(output);out.mkdir();lock=(runner.RESULTS/'phase7-sqlite.lock').open('a+b')
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    record={**original,'case':case.id,'identity':identity,'proof_envelope':case.proof_envelope,'verification_budget_ns':case.verification_budget_ns,'verification_status':'NOT_RUN','verification_wall_ns':None,'performance_reuse':{'receipt':str(prior/'receipt.json'),'sha256':runner.digest(prior/'receipt.json'),'new_performance_samples':0,'original_identity':original['identity'],'scope':'original performance command/source/cold/cleanup and binary/fixture/observer unchanged; only independent proof deadline prospectively changes'}}
    try:
        # This compilation seal covers native source/examples/config, unchanged by
        # the separate proof-supervision ruling. Reuse only exact existing binaries.
        baseline=runner.ROOT/'target/phase7-baseline/layerfs'
        current=build(baseline,'baseline',out,runner,{})
        if current['status']!='PASS' or current['compilation_seal']!=original['build']['compilation_seal'] or current['binaries']!=original['build']['binaries']:raise ValueError('native shared performance vehicle changed')
        record['performance_reuse']['matched_compilation_seal']=current['compilation_seal']
        request=json.loads((prior/'proof-request.json').read_text());request.update(out=str(out),identity=identity)
        runner.write_json(out/'proof-request.json',request)
        scratch=out/'scratch';scratch.mkdir()
        env={**os.environ,**history.ENV,'LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch)}
        result=invoke([sys.executable,str(runner.ROOT/'core/benchmark/fs-bench-pro/shared/phase7_history_proof.py'),'--request',str(out/'proof-request.json')],out,'verifier',case.verification_budget_ns,env,runner.ROOT)
        record['verification']=result;record['verification_wall_ns']=result['wall_ns']
        record['verification_status']='PASS' if result['exit_code']==0 and not result['timed_out'] and isinstance(result['child'],dict) and result['child'].get('status')=='CHECKED' else 'FAIL'
    finally:
        runner.write_json(out/'receipt.json',record)
        if record['verification_status']=='PASS':
            native=record['verification']['child'];runner.write_json(out/'proof.json',native)
            census=json.loads((out/'census.json').read_text());pins=proof.root_pins(record,census,native,out/'receipt.json',out/'census.json',out/'proof.json');runner.write_json(out/'root-pins.json',pins)
        runner.manifest_run(out);lock.close()
    return out
