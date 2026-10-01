#!/usr/bin/env python3
"""One-shot host prototype collector. No LayerFS family or product speed claim."""
import argparse, fcntl, hashlib, json, os, platform, shutil, subprocess, sys, time, tomllib
from pathlib import Path
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[2]
CASES=['namespace-128','namespace-10000','namespace-100000','deep-270','lifecycle-reopen-128','lifecycle-persistent-128','extent-repeated-4097','extent-append-512','extent-dispersed-512','extent-fragmented-8192','extent-truncate-regrow','generation-10000','overlap-1','overlap-2','overlap-3']
def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    return h.hexdigest()
def git(*a):return subprocess.check_output(['git',*a],cwd=ROOT,text=True).strip()
def collect(output,masters):
    binary=BASE/'target/release/phase6-metadata-probe'
    assert binary.is_file(),binary
    dirty=git('status','--porcelain','--untracked-files=normal')
    assert not dirty,f'source must be committed before collection: {dirty}'
    assert not output.exists(),'fresh output required'
    output.mkdir(parents=True)
    masters.mkdir(parents=True,exist_ok=True)
    locks=ROOT/'benchmark-results/phase6-metadata'
    lock=(locks/'measurement.lock').open('a')
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    core=tomllib.loads((ROOT/'core/Cargo.lock').read_text())
    probe=tomllib.loads((BASE/'Cargo.lock').read_text())
    index={(p['name'],p['version'],p.get('checksum')) for p in core['package']}
    for p in probe['package']:
        if p.get('source'):
            assert (p['name'],p['version'],p.get('checksum')) in index,p['name']
    ident={'source':git('rev-parse','HEAD'),'tree':git('rev-parse','HEAD^{tree}'),'spec_commit':'15b2ca92091188b4282a0d104ec50985933d6ea0','binary_sha256':sha(binary),'lock_sha256':sha(BASE/'Cargo.lock'),'oracle_sha256':sha(BASE/'verify.py'),'collector_sha256':sha(Path(__file__)),'schema_sha256':sha(BASE/'schema.sql'),'build_flags_sha256':sha(ROOT/'.cargo/config.toml'),'build_profile':'release --locked; repository-root aarch64 flags','host':platform.platform(),'cache_contract':'uncontrolled OS cache; INELIGIBLE','sample_count':1,'construction_producers':1,'clone_method':'independent shutil.copyfile byte copy; no cold claim','dependency_parity':'PASS'}
    (output/'identity.json').write_text(json.dumps(ident,indent=2)+'\n')
    summary=[{'case':case,'status':'NOT_RUN'} for case in CASES]
    (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    for case_index,case in enumerate(CASES):
        dest=output/case;dest.mkdir()
        master=masters/(case+'.sqlite');seal=master.with_suffix('.seal.json')
        setup=time.monotonic_ns()
        prepared=False
        if not master.exists():
            result=subprocess.run([str(binary),'prepare',case,str(master)],text=True,capture_output=True,timeout=15)
            (dest/'prepare.stdout').write_text(result.stdout)
            (dest/'prepare.stderr').write_text(result.stderr)
            assert result.returncode==0,(case,result.stderr)
            seal.write_text(json.dumps({'source':ident['source'],'binary':ident['binary_sha256'],'database':sha(master)})+'\n')
            prepared=True
        sealed=json.loads(seal.read_text())
        assert sealed['binary']==ident['binary_sha256'] and sealed['database']==sha(master)
        setup_ns=time.monotonic_ns()-setup
        copying=time.monotonic_ns();shutil.copyfile(master,dest/'sample.sqlite');copy_ns=time.monotonic_ns()-copying
        started=time.monotonic_ns()
        command=[str(binary),'run',case,str(dest/'sample.sqlite'),str(dest)]
        row={'case':case,'status':'NOT_RUN','command':command,'setup_ns':setup_ns,'setup_fresh':prepared,'master_sha256':sealed['database'],'copy_ns':copy_ns,'performance_claim':False,'cache_verdict':'INELIGIBLE','sample_count':1}
        try:
            run=subprocess.run(command,text=True,capture_output=True,timeout=15)
            row['complete_command_ns']=time.monotonic_ns()-started
            row['exit_code']=run.returncode
            (dest/'stdout.txt').write_text(run.stdout);(dest/'stderr.txt').write_text(run.stderr)
            row['status']='COMPLETE' if run.returncode==0 else 'FAIL'
        except subprocess.TimeoutExpired as exc:
            row['complete_command_ns']=time.monotonic_ns()-started;row['status']='TIMEOUT'
            (dest/'timeout.json').write_text(json.dumps({'error':str(exc)})+'\n')
        if row['status']=='COMPLETE':
            operation=json.loads((dest/'operation.json').read_text())
            row['operation']=operation
            verify_command=[sys.executable,str(BASE/'verify.py'),case,str(dest)]
            try:
                checked=subprocess.run(verify_command,text=True,capture_output=True,timeout=9.5)
                (dest/'verify.stdout').write_text(checked.stdout);(dest/'verify.stderr').write_text(checked.stderr)
                row['verification']=json.loads(checked.stdout)
                assert checked.returncode==0 or row['verification']['status']=='FAIL'
            except subprocess.TimeoutExpired as exc:row['verification']={'status':'TIMEOUT','reason':str(exc)}
            sidecars={p.name:p.stat().st_size for p in dest.glob('sample.sqlite-*')}
            row['retained_sidecars']=sidecars
            row['resource_observation']={'physical_containment':'UNAVAILABLE','OS_file_cache':'UNAVAILABLE','sqlite_highwater_scope':'operation-reset engine allocations only; not total process memory'}
            row['cleanup']='PASS' if not sidecars and operation['capture_rows']==0 else 'FAIL'
        (dest/'receipt.json').write_text(json.dumps(row,indent=2)+'\n')
        summary[case_index]=row
        (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
        print(case,row['status'],row.get('verification',{}).get('status','NOT_RUN'),flush=True)
    manifests={str(p.relative_to(output)):sha(p) for p in output.rglob('*') if p.is_file() and p.name!='manifest.json'}
    (output/'manifest.json').write_text(json.dumps(manifests,indent=2)+'\n')
    fcntl.flock(lock,fcntl.LOCK_UN)
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--masters',type=Path,required=True)
    args=parser.parse_args();collect(args.output.resolve(),args.masters.resolve())
