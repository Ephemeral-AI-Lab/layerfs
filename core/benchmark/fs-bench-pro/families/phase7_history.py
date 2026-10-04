"""Sole-runner retained-history binding; no alternate producer or relaxed proof."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from diagnostics.history_reference_vehicle import generate, generate_verifier
from families import history_retention as history
from shared import cold_native, history_observer, phase7_history_proof as proof


def build(root, arm, out, common, vehicle):
    from families.phase7_sqlite import invoke, archive
    target = root / 'core/target'
    temporary = []
    examples = ['benchmark_history', 'verify_history']
    sources = {}
    if arm == 'baseline':
        directory = root / 'core/crates/layerfs-api/sdk/examples'
        driver, driver_seals = generate(common.ROOT)
        helper_path = directory / 'history_reference_verify_support.rs'
        verifier, helper, verifier_seals = generate_verifier(common.ROOT, helper_path)
        sources = {'history_reference.rs': driver, 'history_reference_verify.rs': verifier,
                   'history_reference_verify_support.rs': helper}
        vehicle.update(driver_seals=driver_seals, verifier_seals=verifier_seals)
        examples = ['history_reference', 'history_reference_verify']
    compilation = list((root/'core/crates').glob('**/src/**/*.rs')) + list((root/'core/crates').glob('**/sql/**/*.sql'))
    compilation += list((root/'core/crates').glob('**/Cargo.toml')) + list((common.ROOT/'core/crates/layerfs-project/examples').glob('**/*.rs'))
    compilation += [root/'core/Cargo.toml', root/'core/Cargo.lock', root/'.cargo/config.toml']
    seal = hashlib.sha256()
    for path in sorted(compilation):
        seal.update(str(path.relative_to(root) if path.is_relative_to(root) else path.relative_to(common.ROOT)).encode()+b'\0'); seal.update(path.read_bytes())
    for name, text in sorted(sources.items()): seal.update(name.encode()+b'\0'+text.encode())
    compilation_seal = seal.hexdigest()
    cache = root/f'benchmark-results/fs-bench-pro/history-build-{arm}.json'
    if cache.exists():
        prior = json.loads(cache.read_text())
        if prior.get('compilation_seal') == compilation_seal and all(common.digest(Path(b['path'])) == b['sha256'] for b in prior['binaries'].values()):
            return {**prior, 'mode': 'sealed-binary-reuse', 'wall_ns': 0, 'command': None}
    try:
        for name, text in sources.items():
            path = directory/name
            if path.exists(): raise ValueError('reference temporary vehicle occupied')
            path.write_text(text); temporary.append(path)
        command = ['cargo', '+1.85.1', 'build', '--manifest-path', 'core/Cargo.toml', '--release', '--locked', '-p', 'layerfs-project' if arm == 'candidate' else 'layerfs-sdk']
        for example in examples: command += ['--example', example]
        result = invoke(command, out, 'build', 30_000_000_000, {**os.environ, 'CARGO_TARGET_DIR': str(target)}, root)
        result.update(profile='release/locked', compilation_seal=compilation_seal, mode='incremental-build', dependency_reuse='worktree-local target')
        result['status'] = 'PASS' if result['exit_code'] == 0 and not result['timed_out'] else 'FAIL'
        if result['status'] == 'PASS':
            result['binaries'] = {name: archive(target/'release/examples'/name, root, common) for name in examples}
            cache.parent.mkdir(parents=True, exist_ok=True); common.write_json(cache, result)
        return result
    finally:
        for path in reversed(temporary): path.unlink()


def boundaries(stderr, states, arm):
    rows = [json.loads(line.removeprefix('HISTORY_COLD_BOUNDARY ')) for line in stderr.splitlines() if line.startswith('HISTORY_COLD_BOUNDARY ')]
    if [r['before_state'] for r in rows] != list(range(2, states+2)):
        raise ValueError('history cold boundaries missing or out of order')
    minimum_files = 1 if arm == 'candidate' else 2
    if any(r['attestation']['resident_after'] != 0 or r['attestation']['files'] < minimum_files for r in rows):
        raise ValueError('history database boundary residency INELIGIBLE')
    totals = [json.loads(line.removeprefix('HISTORY_COLD_TOTAL ')) for line in stderr.splitlines() if line.startswith('HISTORY_COLD_TOTAL ')]
    if len(totals) != 1 or totals[0] != {'checks': states, 'wall_ns': sum(r['wall_ns'] for r in rows)}:
        raise ValueError('history cold boundary total mismatch')
    return {'status': 'PASS', 'boundaries': rows, 'total': totals[0], 'scope': 'database/extant-WAL content pages; metadata/SHM/internal bounded product buffers excluded'}


def run(case, output, arm, baseline_root, common, corpus_root=None, reference_pins=None):
    from families.phase7_sqlite import invoke, BASE, PROFILE_IDS, REQUIRED_BY_PROFILE, GROUP_ROW_CASES, CASES
    if arm not in ('baseline', 'candidate'): raise ValueError('explicit history arm required')
    if arm == 'candidate' and reference_pins is None:
        raise ValueError('history candidate requires qualified reference pins before build/setup/sample')
    if os.uname().sysname != 'Darwin': raise ValueError('history cold/SQLite profile requires macOS')
    root = common.ROOT if arm == 'candidate' else Path(baseline_root).resolve()
    if arm == 'baseline' and (not root.is_relative_to(common.ROOT/'target/phase7-baseline') or subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip() != BASE or subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)):
        raise ValueError('history reference requires clean pinned owned checkout')
    out = common.owned(output); out.mkdir(parents=True)
    row = {17:'history-stride10',53:'history-stride3',157:'history-stride1'}[case.states]
    record = {'schema':'phase7-sqlite-history-v1','case':case.id,'workload_row':row,'arm':arm,'status':'NOT_RUN','sample_count':0,
              'cache_status':'INCOMPLETE','verification_status':'NOT_RUN','cleanup':{'status':'NOT_RUN'},
              'requested_profile':case.profile,'profile':PROFILE_IDS[case.profile] if arm == 'candidate' else 'phase4.5-memory-off',
              'command_budget_ns':case.command_budget_ns,'verification_budget_ns':case.verification_budget_ns,'proof_policy':case.proof_policy,
              'required_case_ids':tuple(name for name in GROUP_ROW_CASES if CASES[name].profile==case.profile) if case.pack_layout=='group-rows' else REQUIRED_BY_PROFILE[case.profile],'construction_workers':1,
              'comparison_scope':'Corpus open through all real retained-state construction/save/C5, measured cold boundaries, final custody/checkpoint/close and canonical census',
              'margin_arithmetic':'10*candidate_ns<=11*baseline_ns','allocation_ceiling':case.storage_ceiling,
              'observer_status':'NOT_RUN', 'cache_contract':'history-source-cold-and-database-state-boundaries-v1'}
    locks=[]
    try:
        for path in [common.RESULTS/'phase7-sqlite.lock'] + ([root/'target/phase7-sqlite.lock'] if arm == 'baseline' else []):
            path.parent.mkdir(parents=True,exist_ok=True); handle=path.open('a+b'); fcntl.flock(handle,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(handle)
        identity=common.identities()
        if identity['source_dirty']: raise ValueError('freeze committed source/harness before a history arm')
        record['identity']=identity
        if arm == 'candidate':
            pins=json.loads(Path(reference_pins).read_text()); source=proof.validate_pins(pins,identity)
            reference=source['receipt']
            if reference['case'] != case.id or reference['workload_row'] != row: raise ValueError('history reference selection mismatch')
            common.write_json(out/'reference-pins.json',pins); record['reference_pins_sha256']=common.digest(out/'reference-pins.json')
        corpus=Path(corpus_root or history.corpus.DEFAULT_ROOT).resolve()
        record['fixture']=history.corpus.identity(corpus)
        record['setup_method']='reuse original pinned immutable corpus; no prepared Store or product work shifted to setup'
        record['measured_source_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
        record['dependency_seal']=common.digest(root/'core/Cargo.lock'); record['root_cargo_config_sha256']=common.digest(root/'.cargo/config.toml')
        record['competing_work']=common.competing_work(); record['vehicle']={}
        record['build']=build(root,arm,out,common,record['vehicle'])
        if record['build']['status'] != 'PASS': record['reason']='release build failed/exceeded30s';return out
        if arm == 'baseline' and subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True): raise ValueError('reference checkout mutated')
        record['cold_helper']=cold_native.build(common.ROOT,out,invoke)
        record['observer']=history_observer.build(common.ROOT,out,invoke,common)
        record['compilation_seal']=record['build']['compilation_seal']
        source_files=list((root/'core/crates').glob('**/src/**/*.rs'))+list((root/'core/crates').glob('**/sql/**/*.sql'))
        value=hashlib.sha256()
        for path in sorted(source_files):value.update(str(path.relative_to(root)).encode()+b"\0"+path.read_bytes())
        record['product_seal']=value.hexdigest()
        scratch=out/'scratch';scratch.mkdir();db=out/'store.sqlite'
        env={**os.environ,**history.ENV,'TMPDIR':str(scratch),'LAYERFS_HISTORY_CURSOR_KEY':'28'*32,
             'LAYERFS_HISTORY_COLD_HELPER':record['cold_helper']['binary'],
             'DYLD_INSERT_LIBRARIES':record['observer']['path'],'LAYERFS_SQLITE_WORK_OUTPUT':str(out/'sql-work.json'),
             'LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json'),'LAYERFS_CLOSE_OBSERVER_OUTPUT':str(out/'close.json')}
        binaries=record['build']['binaries']
        driver=binaries['benchmark_history' if arm=='candidate' else 'history_reference']['path']
        command=[driver,str(corpus),str(db),str(scratch/'ordering'),row,'complete',case.profile]
        if arm=='candidate' and case.pack_layout=='group-rows':command.append('group-rows')
        key=hashlib.sha256(json.dumps([case.id,arm,identity['source_tree'],identity['harness_seal'],record['measured_source_commit'],record['fixture']['manifest_sha256'],record['observer']['seal']],sort_keys=True).encode()).hexdigest()
        claim=common.RESULTS/'phase7-history-sample-claims'/key;claim.parent.mkdir(parents=True,exist_ok=True)
        with claim.open('x') as file:file.write(str(out)+'\n')
        start=time.monotonic_ns()
        record['residency']=cold_native.attest_paths([corpus/'checkpoint-manifest.json',corpus/'inputs',corpus/'oracles'],record['cold_helper'],out,case.command_budget_ns,invoke,common.ROOT)
        if record['residency']['status']!='PASS':record.update(status='INELIGIBLE',cache_status='INELIGIBLE');return out
        remaining=case.command_budget_ns-(time.monotonic_ns()-start)
        if remaining<=0:record['reason']='cold attestation exhausted command budget';return out
        record['performance']=invoke(command,out,'driver',remaining,env,root);record['sample_count']=1
        sample=record['performance'];child=sample['child']
        stderr=(out/'driver.stderr').read_text()
        from shared.sqlite_contract import allocations
        record['storage']=allocations([db] if arm=='candidate' else [db,Path(str(db)+'.history.sqlite')]);record['storage_bytes']=record['storage']['total_bytes']
        ordering=scratch/'ordering'
        clean=(not ordering.exists() or not list(ordering.iterdir())) and not list(out.glob('.layerfs-allocation-*'))
        record['cleanup']={'status':'PASS' if clean else 'FAIL','scope':'child exited; ordering/allocation scratch empty; original databases retained'}
        record['command_wall_ns']=time.monotonic_ns()-start
        if sample['exit_code'] or sample['timed_out'] or not isinstance(child,dict) or child.get('status')!='COMPLETE':
            record['status']='INELIGIBLE' if 'INELIGIBLE' in stderr else 'FAIL';return out
        record['comparison_ns']=child['operation_ns'];record['status']='COMPLETE'
        record['database_cold']=boundaries(stderr,case.states,arm);record['cache_status']='PASS'
        record['observations']=history_observer.collect(out);record['observer_status']='CHECKED'
        stages=[json.loads(line.removeprefix('HISTORY_STATE_WORK ')) for line in stderr.splitlines() if line.startswith('HISTORY_STATE_WORK ')]
        if len(stages)!=case.states:raise ValueError('history state work receipt incomplete')
        record['state_work']=stages
        if arm=='candidate':
            settings=[json.loads(line.removeprefix('EFFECTIVE_PROFILE ')) for line in stderr.splitlines() if line.startswith('EFFECTIVE_PROFILE ')]
            expected={'identity':PROFILE_IDS[case.profile],'journal_mode':'wal' if case.profile=='durable' else 'memory','synchronous':2 if case.profile=='durable' else 0,'foreign_keys':1,'fullfsync':1 if case.profile=='durable' else 0,'checkpoint_fullfsync':1,'page_size':4096,'cache_size':-2048,'mmap_size':0,'temp_store':2,'wal_checkpoint_performed':case.profile=='durable'}
            if settings!=[expected]:raise ValueError('history actual profile/completion mismatch')
            record['effective_profile']=settings[0]
            layouts=[json.loads(line.removeprefix('EFFECTIVE_PACK_LAYOUT ')) for line in stderr.splitlines() if line.startswith('EFFECTIVE_PACK_LAYOUT ')]
            if layouts!=[{'layout':'GroupRows' if case.pack_layout=='group-rows' else 'Monolithic'}]:raise ValueError('actual SQLite pack layout mismatch')
            record['effective_pack_layout']=layouts[0]
        request={'observer':record['observer'],'proof_policy':case.proof_policy,'out':str(out),'arm':arm,'producer':child,'db':str(db),'row':row,'verifier':binaries['verify_history' if arm=='candidate' else 'history_reference_verify']['path'],'corpus':str(corpus),'profile':case.profile,'identity':identity,'pins':str(out/'reference-pins.json'),'sqlite_schema_version':2 if case.pack_layout=='group-rows' else 1}
        common.write_json(out/'proof-request.json',request)
        proof_env={**os.environ,**history.ENV,'LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch)}
        verification=invoke([sys.executable,str(common.ROOT/'core/benchmark/fs-bench-pro/shared/phase7_history_proof.py'),'--request',str(out/'proof-request.json')],out,'verifier',case.verification_budget_ns,proof_env,common.ROOT)
        record['verification']=verification;record['verification_wall_ns']=verification['wall_ns']
        record['verification_status']='PASS' if verification['exit_code']==0 and not verification['timed_out'] and isinstance(verification['child'],dict) and verification['child'].get('status')=='CHECKED' else 'FAIL'
        return out
    except Exception as error:
        record['status']='INCOMPLETE'; record['reason']=str(error); raise
    finally:
        common.write_json(out/'receipt.json',record)
        if arm=='baseline' and record.get('verification_status')=='PASS' and record.get('status')=='COMPLETE' and record.get('command_wall_ns',case.command_budget_ns+1)<=case.command_budget_ns and record.get('cleanup',{}).get('status')=='PASS':
            census=json.loads((out/'census.json').read_text()); native=record['verification']['child']
            common.write_json(out/'proof.json',native)
            pins=proof.root_pins(record,census,native,out/'receipt.json',out/'census.json',out/'proof.json')
            common.write_json(out/'root-pins.json',pins)
        common.manifest_run(out)
        for handle in reversed(locks):handle.close()
