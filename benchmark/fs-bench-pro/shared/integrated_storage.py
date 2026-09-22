"""Ordinary schema10 live smoke and retained issue103 evidence readers."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import time
import uuid

import runner
import runtime
import isolation

CONTRACT = 'docs/roadmap/0.1/0.1.5/issue103/stride3-integrated-compaction-v1.md'
SCENARIO = 'deepseek-stride3-integrated-compaction-v1'
PROFILES = {
    'deepseek-stride3': {'contract': CONTRACT, 'scenario': SCENARIO,
        'indices': tuple(range(1,158,3)), 'checkpoint_map': {65:67,57:58},
        'access_profile': 'historical-access-stride3-integrated-v1', 'case_suffix': '-s3-v1'},
    'deepseek-full': {'contract': 'docs/roadmap/0.1/0.1.5/issue103/full157-integrated-compaction-v1.md',
        'scenario': 'deepseek-full157-integrated-compaction-v1',
        'indices': tuple(range(1,158)), 'checkpoint_map': {},
        'access_profile': 'historical-access-full157-integrated-v1', 'case_suffix': '-f157-v1'},
}
ORDINARY_FULL = {'contract': 'docs/roadmap/0.1/0.1.5/full157-execution-contract.md',
    'indices': tuple(range(1,158)), 'checkpoint_map': {},
    'access_profile': 'historical-access-full157-ordinary-v1', 'case_suffix': '-f157-ordinary-v1'}


def save(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2, sort_keys=True); stream.write('\n')


def records(raw):
    return [json.loads(line) for line in raw.decode().splitlines() if line.strip()]


def freeze(path):
    path = Path(path).resolve()
    files = {}
    for file in sorted(path.parent.rglob('*')):
        if file.is_symlink():
            raise ValueError('measured Store contains a symlink')
        if file.is_file():
            m = file.stat()
            files[str(file.relative_to(path.parent))] = {'sha256': runtime.file_sha256(file),
                'allocated_bytes': m.st_blocks*512, 'apparent_bytes': m.st_size,
                'inode': m.st_ino, 'device': m.st_dev}
    if set(files) != {path.name}:
        raise ValueError('measured Store has unexpected sidecars or temporary files')
    return {'path': str(path), 'sha256': files[path.name]['sha256'], 'files': files,
            'allocated_bytes': sum(f['allocated_bytes'] for f in files.values()),
            'apparent_bytes': sum(f['apparent_bytes'] for f in files.values()),
            'frozen_at_ns': time.time_ns()}


def check_identity(binary,image):
    current=runner.source_build_args()
    identity=json.loads(Path(str(binary)+'.identity.json').read_text())
    info=runner.image_info(image,time.monotonic()+30);labels=info['Config']['Labels']
    if identity['binary_sha256']!=runtime.file_sha256(binary) or identity['LAYERFS_SOURCE_SEAL']!=current['LAYERFS_SOURCE_SEAL'] or labels['dev.layerfs.source-seal']!=current['LAYERFS_SOURCE_SEAL'] or labels['dev.layerfs.product-seal']!=current['LAYERFS_PRODUCT_SEAL']:
        raise ValueError('stale host/image/source identity')
    probe=identity.get('integrated_format_probe',{})
    if probe.get('status')!='PASS' or probe.get('storage_policy')!='ordinary': raise ValueError('ordinary linked-format probe missing')
    return current,identity,info


def smoke(args):
    output=Path(args.integration_smoke).resolve();output.mkdir(parents=True)
    with isolation.worktree_lock_path().open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        current,identity,info=check_identity(args.host_binary,args.image)
        save(output/'identity.json',{'schema':'ordinary-storage-integration-smoke-v1','source':current,'host_identity':identity,'image_id':info['Id'],'contract_sha256':runtime.file_sha256(runner.REPO/'docs/roadmap/0.1/0.1.5/compaction-removal.md')})
        sample=None;result={'status':'INCOMPLETE','admission_eligible':False}
        host=output/'host-runtime';host.mkdir();tmp=host/'tmp';tmp.mkdir()
        try:
            sample=runtime.start_sample(info['Id'],'layerfs-i103-'+uuid.uuid4().hex[:12],{'family':'ordinary-storage-integration-smoke-v1','run':output.name},deadline=runtime.Deadline.after(120))
            command=[args.host_binary,'storage-integration-smoke',str(host),sample.id]
            result['command']=command
            raw=runtime.run(command,deadline=runtime.Deadline.after(300),env={**os.environ,'TMPDIR':str(tmp),'SQLITE_TMPDIR':str(tmp),'LAYERFS_EXEC_TRANSPORT':'daemon','LAYERFS_FUSE_TRANSPORT':'daemon'},check=False,output_limit=8*1024**2)
            (output/'stdout.jsonl').write_bytes(raw.stdout);(output/'stderr.log').write_bytes(raw.stderr)
            rows=records(raw.stdout);result.update(records=rows,exit_code=raw.returncode,timed_out=raw.timed_out)
            if raw.returncode or raw.timed_out or not any(r.get('kind')=='storage-integration-smoke' and r.get('status')=='PASS' for r in rows): raise ValueError('live integration smoke failed')
            result['status']='PASS'
        except Exception as error: result.update(error_type=type(error).__name__,error=str(error))
        finally:
            if sample:
                try:
                    logs=runtime.run(['docker','logs',sample.id],deadline=runtime.Deadline.after(30),check=False)
                    (output/'container.log').write_bytes(logs.stdout+logs.stderr)
                    sample.remove(runtime.Deadline.after(120));result['cleanup_status']='PASS'
                except Exception as error:result.update(status='INCOMPLETE',cleanup_status='FAIL',cleanup_error=str(error))
            save(output/'result.json',result)
        print(json.dumps({'status':result['status'],'output':str(output)}),flush=True)
        return 0 if result['status']=='PASS' and result.get('cleanup_status')=='PASS' else 1


def access_performance(run, profile):
    identity=json.loads((run/'identity.json').read_text())
    folder=run/identity['smoke']
    performance=json.loads((folder/'performance-result.json').read_text())
    if json.loads((run/'performance-summary.json').read_text())['status']!='PASS' or performance['status']!='PASS' or performance.get('cleanup_status')!='PASS':
        raise ValueError('complete history performance and cleanup required')
    rows=performance['records']; count=len(profile['indices'])
    if len(rows)!=count or tuple(r['full157_index'] for r in rows)!=profile['indices'] or [r['index'] for r in rows]!=list(range(1,count+1)):
        raise ValueError('retained checkpoint mapping')
    if any(r['identity']!=(r['commit_id'] or 'initial') for r in rows):
        raise ValueError('produced checkpoint identity mismatch')
    return identity,folder,performance


def ordinary_access_profile(identity):
    if identity.get('storage_compact') is not False or identity['smoke']!='deepseek-full':
        raise ValueError('ordinary full157 producer required')
    if identity['full_run_contract_sha256']!=runtime.file_sha256(runner.REPO/ORDINARY_FULL['contract']):
        raise ValueError('prospective full157 contract changed')
    return ORDINARY_FULL


def freeze_access(run):
    run=Path(run).resolve()
    profile=ordinary_access_profile(json.loads((run/'identity.json').read_text()))
    identity,folder,performance=access_performance(run,profile)
    if list(run.glob('verification-*')) or list(folder.glob('verification-*')):
        raise ValueError('freeze measured ordinary Store before verification starts')
    source=folder/'host-runtime/store.sqlite'
    measured=json.loads((run/'performance-manifest.json').read_text())
    expected=measured.get(str(source.relative_to(run)))
    if not expected or runtime.file_sha256(source)!=expected:
        raise ValueError('measured ordinary Store changed before freeze')
    observation=runner.sdk_store_observation(source)
    archive=folder/'frozen-measured-store'; archive.mkdir()
    copied=runtime.closed_store_copy(source,archive/'store.sqlite',deadline=runtime.Deadline.after(120))
    frozen=freeze(archive/'store.sqlite')
    if frozen['sha256']!=expected: raise ValueError('frozen ordinary Store identity mismatch')
    save(folder/'access-freeze.json',{'schema':'ordinary-history-access-freeze-v1','status':'PASS',
        'identity_sha256':runtime.file_sha256(run/'identity.json'),
        'history_result_sha256':runtime.file_sha256(folder/'performance-result.json'),
        'performance_manifest_sha256':runtime.file_sha256(run/'performance-manifest.json'),
        'branch_id':(folder/'host-runtime/branch-id').read_text().strip(),
        'measured_store':{**observation,'sha256':expected},'frozen_store':frozen,'copy':copied})
    print(json.dumps({'status':'PASS','store':str(archive/'store.sqlite'),'states':len(performance['records'])}),flush=True)
    return 0


def prepare_access(run, destination, data):
    import hashlib
    run=Path(run).resolve(); destination=Path(destination).resolve(); data=Path(data)
    identity=json.loads((run/'identity.json').read_text())
    compact=identity.get('storage_compact') is True
    if compact and identity['smoke'] not in PROFILES: raise ValueError('registered integrated producer required')
    profile=PROFILES[identity['smoke']] if compact else ordinary_access_profile(identity)
    identity,folder,performance=access_performance(run,profile); count=len(profile['indices'])
    if json.loads((run/'verification-summary.json').read_text())['status']!='PASS': raise ValueError('complete history verification must finish first')
    verification=json.loads((folder/'verification-result.json').read_text())
    if len(verification['records'])!=count or verification['status']!='PASS' or verification.get('cleanup_status')!='PASS': raise ValueError('exact state verification incomplete')
    for observed,produced in zip(verification['records'],performance['records']):
        if observed['status']!='PASS' or observed['index']!=produced['index'] or observed['identity']!=produced['identity']:
            raise ValueError('state verification identity mismatch')
    master=folder/'frozen-measured-store/store.sqlite'
    if compact:
        if identity['integrated_scenario']!=profile['scenario'] or runtime.file_sha256(runner.REPO/profile['contract'])!=identity['integrated_contract_sha256']:
            raise ValueError('prospective contract changed')
        compaction=json.loads((folder/'compaction-result.json').read_text())
        if compaction['status']!='PASS' or runtime.file_sha256(master)!=compaction['measured_store']['sha256']: raise ValueError('measured frozen image mismatch')
        custody={'compaction_result_sha256':runtime.file_sha256(folder/'compaction-result.json')}
        branch_id=(folder/'host-runtime/branch-id').read_text().strip()
    else:
        frozen=json.loads((folder/'access-freeze.json').read_text())
        bindings={'identity_sha256':run/'identity.json','history_result_sha256':folder/'performance-result.json',
            'performance_manifest_sha256':run/'performance-manifest.json'}
        if frozen['status']!='PASS' or any(frozen[key]!=runtime.file_sha256(path) for key,path in bindings.items()):
            raise ValueError('ordinary access performance custody changed')
        observed=freeze(master)
        if any(observed[key]!=frozen['frozen_store'][key] for key in ('sha256','files','allocated_bytes','apparent_bytes')):
            raise ValueError('measured frozen ordinary Store changed')
        custody={'access_freeze_sha256':runtime.file_sha256(folder/'access-freeze.json'),
            'measured_store':frozen['measured_store'],
            'verification_store':{**runner.sdk_store_observation(folder/'host-runtime/store.sqlite'),
                'sha256':runtime.file_sha256(folder/'host-runtime/store.sqlite')}}
        branch_id=frozen['branch_id']
    indexed={r['full157_index']:r for r in performance['records']}
    if tuple(indexed)!=profile['indices'] or [r['index'] for r in performance['records']]!=list(range(1,count+1)): raise ValueError('retained checkpoint mapping')
    template=json.loads((runner.BENCH/'families/historical_access/fixture.json').read_text())
    cases=[]
    for old in template['cases']:
        requested=old['full157_index']; index=profile['checkpoint_map'].get(requested,requested)
        row=indexed[index]
        oracle_path=Path(row['oracle'])
        if runtime.file_sha256(oracle_path)!=row['oracle_sha256']: raise ValueError('original oracle changed')
        oracle=json.loads(oracle_path.read_text())
        case={**old,'id':old['id'].replace('-v2',profile['case_suffix']),'template_case':old['id'],
            'requested_original_index':requested,'full157_index':index,'retained_ordinal':row['index'],
            'commit_id':row['commit_id'],'source_commit':row['sha'],'original_oracle_sha256':row['oracle_sha256']}
        if old['operation']=='directory':
            names={bytes.fromhex(path).split(b'/')[0] for path in oracle}
            case['expected']={'names':','.join(name.hex() for name in sorted(names))}
        else:
            mode,size,digest=oracle[old['path'].encode().hex()]
            expected={'mode':mode,'size':size,'mtime':1000000000,'mtime_nsec':0}
            if old['operation']=='read':
                if old['offset']+old['length']>size: raise ValueError('mapped range outside original file')
                rows=[line.split('\t') for line in (Path(row['input'])/'manifest.tsv').read_text().splitlines()]
                entry=next(entry for entry in rows if entry[3]==old['path'].encode().hex())
                raw=runtime.run(['git','--git-dir='+str(data/'source.git'),'cat-file','blob',entry[1]],deadline=runtime.Deadline.after(30),output_limit=2*1024**2).stdout
                if len(raw)!=size or hashlib.sha256(raw).hexdigest()!=digest or hashlib.sha1(b'blob '+str(size).encode()+b'\0'+raw).hexdigest()!=entry[1]: raise ValueError('original access content identity')
                expected['sha256']=hashlib.sha256(raw[old['offset']:old['offset']+old['length']]).hexdigest()
            case['expected']=expected
        if not profile['checkpoint_map'] and case['expected']!=old['expected']: raise ValueError('original full157 access oracle mismatch')
        cases.append(case)
    fixture={'schema':'historical-access-v2','profile':profile['access_profile'],
        'contract_commit':identity['source']['LAYERFS_SOURCE_COMMIT'],'contract_sha256':runtime.file_sha256(runner.REPO/profile['contract']),
        'store_sha256':runtime.file_sha256(master),'store':str(master),
        'branch_id':branch_id,
        'history_result_sha256':runtime.file_sha256(folder/'performance-result.json'),
        'verification_result_sha256':runtime.file_sha256(folder/'verification-result.json'),
        **custody,'cases':cases}
    save(destination,fixture)
    print(json.dumps({'fixture':str(destination),'store':str(master),'cases':len(cases),'status':'PASS'}))
    return 0


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    choice=parser.add_mutually_exclusive_group(required=True)
    choice.add_argument('--integration-smoke')
    choice.add_argument('--prepare-access')
    choice.add_argument('--freeze-access',help='freeze a complete ordinary full157 measured Store before verification')
    parser.add_argument('--output')
    parser.add_argument('--data',default='/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data')
    parser.add_argument('--image')
    parser.add_argument('--host-binary',default=str(runner.REPO/'target/release/fs-benchmark-pro'))
    args=parser.parse_args(argv)
    if args.prepare_access or args.freeze_access:
        if args.prepare_access and not args.output: parser.error('--prepare-access requires --output')
        with isolation.worktree_lock_path().open('a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            if args.freeze_access: return freeze_access(args.freeze_access)
            return prepare_access(args.prepare_access,args.output,args.data)
    if not args.image: parser.error('--integration-smoke requires --image')
    return smoke(args)

if __name__=='__main__':raise SystemExit(main())
