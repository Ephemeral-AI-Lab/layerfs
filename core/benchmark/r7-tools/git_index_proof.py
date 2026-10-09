"""Real SDK functional proof of selected-index parsing and exact L byte survival.

Setup is separate (at most 360 s); functional work is at most 100 s; outer stop
460 s. Commands/verifiers have 9.5 s, mount/Commit/unmount 15 s, cleanup 5 s.
No performance, native comparison, cache qualification, Init or retry occurs.
Failures retain the original container/Exec/mount custody; only this host
controller is fenced. The lead owns explicit container disposition.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import time

from prepare_inputs import owned
from oracle_prepare import ReadFile, OutputFile

REPOSITORY = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPOSITORY / "core/benchmark/fs-bench-pro"))
from r7 import deployment, git_index_oracle, workloads
from r7.runner import EventProcess, OriginalFailure

IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"
SCHEMA = "r7-real-git-index-survival-proof-v1"
DEFAULTS = {
    "core.repositoryformatversion": "0", "extensions.objectformat": "sha1",
    "extensions.worktreeconfig": "false", "core.filemode": "true", "core.ignorecase": "false",
    "index.skiphash": "false", "core.splitindex": "false", "core.sparsecheckout": "false",
    "core.untrackedcache": "keep", "core.fsmonitor": "false", "feature.manyfiles": "false",
}

# Executed only by the lead's eventual proof, through ordinary unregistered SDK
# Bash. stdout is delivered to the runtime's file owner without a 64 KiB cap.
PROBE = r'''
import hashlib, importlib.util, json, os, shutil, subprocess, sys, time
from pathlib import Path
helper = Path('/code/git_index_oracle.py')
class OwnedFile:
    def __init__(self,path,mode):
        self.stream=Path(path).open(mode,buffering=0)
    def __enter__(self):
        return self.stream
    def __exit__(self,kind,original,traceback):
        try:
            self.stream.close()
        except OSError as closing:
            if original is None:
                raise
            original.independent_close_failures=[*getattr(original,'independent_close_failures',[]),str(closing)]
        return False
def read_bytes(path):
    with OwnedFile(path,'rb') as source:
        return source.read()
with OwnedFile(helper,'rb') as source:
    actual_helper = hashlib.sha256(source.read()).hexdigest()
if actual_helper != job['helper_sha256']:
    raise RuntimeError('actual staged Git-index helper bytes differ; not imported')
spec = importlib.util.spec_from_file_location('r7_actual_git_index', helper)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.require((os.getuid(), os.getgid()) == (501,20), 'ordinary nonroot query/verifier identity differs')
m.require(all(os.environ.get(key) == value for key,value in job['environment'].items()), 'original selected environment differs')
root = Path.cwd()
directory = Path(job['inside'])
if job['mode'] == 'query':
    m.require(not os.path.lexists(directory), 'exclusive query evidence already exists')
    directory.mkdir(mode=0o700)
    queries = []
    deadline = time.monotonic() + 9.0
    def query(label, argv, missing=False):
        prefix = directory / label
        out, err = prefix.with_suffix('.stdout'), prefix.with_suffix('.stderr')
        attempt = dict(label=label,argv=argv,attempts=1,stdout=str(out),stderr=str(err),pid=None)
        m.write_new(prefix.with_suffix('.attempt.json'),attempt)
        queries.append(attempt)
        # Real owned files preserve all bytes even if the command fails. A
        # timeout retains its actual child PID, without cancellation or replay.
        with OwnedFile(out,'xb') as stdout, OwnedFile(err,'xb') as stderr:
            child = subprocess.Popen(argv,stdout=stdout,stderr=stderr)
            attempt['pid'] = child.pid
            m.write_new(prefix.with_suffix('.ack.json'),attempt)
            code = child.wait(timeout=max(0.001,deadline-time.monotonic()))
        attempt.update(exit_code=code,stdout_sha256=m.file_digest(out),stderr_sha256=m.file_digest(err))
        m.write_new(prefix.with_suffix('.completion.json'),attempt)
        data, errors = read_bytes(out), read_bytes(err)
        m.require(len(data)<=65536 and not errors, 'unqualified query output; original files retained')
        m.require(code == 0 or missing and code == 1 and not data, 'original Git query refused; no default guessed')
        return data.decode('utf-8').strip() if code == 0 else None
    executable = shutil.which('git')
    m.require(executable is not None, 'actual Git binary unavailable')
    executable = str(Path(executable).resolve(strict=True))
    binary_sha = m.file_digest(executable)
    version = query('version',[executable,'--version'])
    policy = job.get('default_policy')
    m.require(isinstance(policy,dict) and policy.get('schema') == 'r7-git-default-policy-v1'
              and policy.get('version') == version and policy.get('git_binary_sha256') == binary_sha
              and policy.get('defaults') == job['known_defaults']
              and policy.get('primary_sources') and policy.get('review_receipt_sha256'),
              'actual Git version lacks the selected sealed primary-source default policy')
    def seal_text(value):
        return isinstance(value,str) and len(value)==64 and all(character in '0123456789abcdef' for character in value)
    m.require(seal_text(policy['review_receipt_sha256']) and all(
              isinstance(row,dict) and row.get('url','').startswith('https://raw.githubusercontent.com/git/git/')
              and seal_text(row.get('sha256')) for row in policy['primary_sources']),
              'qualified primary-source/default review seals missing')
    object_format = query('actual-object-format',[executable,'-c','core.fsmonitor=false','rev-parse','--show-object-format'])
    m.require(object_format == 'sha1', 'actual Git object format unsupported')
    bools = {'extensions.worktreeconfig','core.filemode','core.ignorecase','index.skiphash',
             'core.splitindex','core.sparsecheckout','core.fsmonitor','feature.manyfiles'}
    values, provenance = {}, {}
    for key in [*m.EFFECTIVE_KEYS,'feature.manyfiles']:
        argv = [executable,'-c','core.fsmonitor=false','config','--get']
        if key in bools:
            argv.append('--type=bool')
        argv.append(key)
        original = query(key.replace('.','-'),argv,True)
        if original is None:
            m.require(key in policy['defaults'], 'absence has no qualified default: '+key)
            value = policy['defaults'][key]
            provenance[key] = dict(original='ABSENT',exit_code=1,documented_default=value)
        else:
            value = original
            provenance[key] = dict(original=original,exit_code=0)
        values[key] = value
    m.require(values['feature.manyfiles']=='false', 'feature.manyFiles implicit skipHash/v4/UNTR defaults unsupported')
    m.require(values['extensions.objectformat']==object_format, 'configuration/default and actual Git object format differ')
    receipt = dict(schema='r7-original-git-configuration-queries-v1',status='OBSERVED',root=str(root),
                   git_binary=executable,git_binary_sha256=binary_sha,git_version=version,queries=queries,
                   original_values=provenance,actual_object_format=object_format,qualified_defaults_policy=policy,
                   command_override={'core.fsmonitor':'false'},environment=job['environment'])
    query_path = directory/'queries.json'
    m.write_new(query_path,receipt)
    effective = {key:values[key] for key in m.EFFECTIVE_KEYS}
    worktree = root/'.git/config.worktree'
    pin = dict(schema=m.PIN_SCHEMA,object_format=effective['extensions.objectformat'],index_versions=[2,3],
               git=dict(binary=executable,sha256=binary_sha,version=version,
                        version_receipt_sha256=queries[0]['stdout_sha256']),
               config_sha256=m.file_digest(root/'.git/config'),
               worktree_config_sha256=m.file_digest(worktree) if os.path.lexists(worktree) else None,
               effective_config=effective,effective_config_sha256=m.sha256(m.canonical(effective)),
               effective_config_receipt_sha256=m.file_digest(query_path))
    m.validate_pin(pin)
    pin_path = directory/'pin.json'
    m.write_new(pin_path,pin)
    print(json.dumps(dict(schema='r7-git-index-original-pin-v1',status='OBSERVED',pin=pin,
                         pin_path=str(pin_path),pin_file_sha256=m.file_digest(pin_path),
                         original_queries=receipt,query_receipt_path=str(query_path),query_receipt_sha256=m.file_digest(query_path)),sort_keys=True))
else:
    pin = m.load_sealed(directory/'pin.json',job['pin_file_sha256'])
    observation = m.observe(root,pin)
    json.dump(observation,sys.stdout,sort_keys=True)
    sys.stdout.write('\n')
'''


def sha(path):
    value = hashlib.sha256()
    with ReadFile(path, "proof_input_hash") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            value.update(block)
    return value.hexdigest()


def write_bytes(path, raw):
    with OutputFile(path, "exclusive_proof_output") as stream:
        for start in range(0, len(raw), 65536):
            part = raw[start:start+65536]
            if stream.write(part) != len(part):
                raise OriginalFailure("short original proof output write; no tail resend")


def write_json(path, value):
    # Finite control receipts; full 14k-entry observations remain in their own
    # runtime stdout artifacts rather than this controller result.
    write_bytes(path,(json.dumps(value,sort_keys=True,indent=2)+'\n').encode())


def read_json(path):
    with ReadFile(path,"original_proof_json_read") as stream:
        return json.load(stream)


def body(output, name, payload):
    path = output/(name+'.sh')
    guarded = 'try:\n'+''.join('    '+line+'\n' for line in PROBE.splitlines())
    guarded += "except BaseException as original:\n"
    guarded += "    print(json.dumps(dict(schema='r7-git-index-probe-failure-v1',status='UNAVAILABLE',original_failure_type=type(original).__name__,original_failure=str(original),independent_close_failures=getattr(original,'independent_close_failures',[]),original_queries=locals().get('queries',[]),evidence_directory=job['inside'],automatic_retry=False,child_cancellation='NOT_ATTEMPTED'),sort_keys=True))\n"
    guarded += "    raise SystemExit(1)\n"
    raw = ('set -euo pipefail\npython3 -B - <<\'R7PY\'\nimport json\njob = '+repr(payload)+'\n'+guarded+'\nR7PY\n').encode()
    write_bytes(path,raw)
    return path


def attempt(runtime, output, result, operation, key, path, seconds, deadline, controls=None):
    sequence = len(result['phases'])+1
    declared = dict(sequence=sequence,operation=operation,key=key,script=str(path) if path else None,
                    script_sha256=sha(path) if path else None,container=runtime.container,
                    attempts=1,wall_stop_seconds=seconds)
    write_json(output/f'{sequence:03}-attempt.json',declared)
    result['current_attempt']=declared
    before = len(runtime.rows)
    event = runtime.send(operation,key,path,deadline=min(deadline,time.monotonic()+seconds))
    fields = event.get('fields',{})
    if controls is not None:
        left,right = fields.get('control_send_records_before'),fields.get('control_send_records_after')
        if type(left) is not int or type(right) is not int or right-left != controls:
            raise OriginalFailure('original control-send range differs: '+operation)
    if path is not None:
        if fields.get('command_sha256') != declared['script_sha256'] or fields.get('registered_execs') != '0' or fields.get('exit_code') != '0':
            raise OriginalFailure('original body SHA/zero registration/known-zero exit missing')
        created = [row for row in runtime.rows[before:] if row.get('event')=='exec_created']
        if len(created)!=1 or created[0].get('fields',{}).get('command_sha256')!=declared['script_sha256']:
            raise OriginalFailure('original acknowledged Exec/body fingerprint missing')
    completed = dict(**declared,original_event=event)
    write_json(output/f'{sequence:03}-completion.json',completed)
    result['phases'].append(completed)
    return event


def stdout_json(event, output):
    path = Path(event['fields']['stdout'])
    if not path.resolve(strict=True).is_relative_to(output):
        raise OriginalFailure('original stdout artifact escapes owned proof output')
    digest = sha(path)
    value = git_index_oracle.load_sealed(path,digest)
    return value, dict(path=str(path),sha256=digest,bytes=path.stat().st_size,
                      scope='original runtime streamed stdout; no 64 KiB/full-index output cap')


def cleanup(runtime, output, result, key, deadline):
    until = min(deadline,time.monotonic()+5)
    while True:
        if time.monotonic()>=until:
            raise TimeoutError('original closed token did not reach Gone within five seconds')
        event = attempt(runtime,output,result,'cleanup',key,None,max(0,until-time.monotonic()),until,1)
        state = event.get('fields',{}).get('state')
        if state=='Gone':
            return
        if state not in ('Live','Held','Queued'):
            raise OriginalFailure('unknown original cleanup disposition')
        time.sleep(min(0.01,max(0,until-time.monotonic())))


def host_fence(runtime, output, cause, result):
    # The shared owner may already have fenced a partial constructor. Reuse its
    # original idempotent disposition; no second signal, wait or operation.
    retained = runtime.retain(cause)
    stopped = any(row.get('event') == 'container_stopped' for row in runtime.rows)
    stop_sent = result.get('current_attempt', {}).get('operation') == 'stop'
    custody = dict(original_failure=str(cause), container=runtime.container, exec_ids=runtime.exec_ids,
                   partial_host_owner=retained,
                   container_stop='KNOWN_ACKNOWLEDGEMENT' if stopped else
                       'ORIGINAL_STOP_SENT; outcome unavailable' if stop_sent else 'NOT_ATTEMPTED; lead owns disposition',
                   filesystem_drain='NOT_ESTABLISHED', operations_replayed=False)
    try:
        write_json(output/'retained-custody.json', custody)
    except Exception as failure:
        custody['independent_receipt_output_failure'] = dict(type=type(failure).__name__, failure=str(failure))
    return custody


def compare_same_l_index(before, after):
    compared = git_index_oracle.compare(before, after)
    if compared['status'] != 'PASS' or before['raw_index_sha256'] != after['raw_index_sha256'] or before['raw_index_bytes'] != after['raw_index_bytes']:
        raise OriginalFailure('exact original L index bytes/metadata did not survive Commit/fresh mount')
    times = [item['filesystem_metadata'] for item in (before, after)]
    if times[0]['mtime_ns'] != times[1]['mtime_ns'] or any(item['ctime_ns'] != item['mtime_ns'] for item in times):
        raise OriginalFailure('same-L index supported mtime or portable ctime=mtime did not survive')
    compared['same_l_supported_times'] = dict(mtime_ns=times[0]['mtime_ns'], portable_ctime_equals_mtime=True,
        scope='exact supported same-L times; cross-incarnation device/inode equality not claimed')
    return compared


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--default-policy',type=Path)
    parser.add_argument('--default-policy-sha256')
    args = parser.parse_args()
    started = time.monotonic()
    outer_deadline = started+459
    config_path = owned(args.config)
    config = read_json(config_path)
    if (args.default_policy is None)!=(args.default_policy_sha256 is None):
        raise OriginalFailure('default-policy path and exact byte seal must be supplied together')
    policy_artifact = None
    if args.default_policy is not None:
        policy_path = owned(args.default_policy)
        config['git_default_policy'] = git_index_oracle.load_sealed(policy_path,args.default_policy_sha256)
        policy_artifact = dict(path=str(policy_path),sha256=args.default_policy_sha256)
    output = owned(args.output,existing=False)
    if output.exists():
        raise OriginalFailure('exclusive proof output already exists')
    if config['identities']['image_id']!=IMAGE or (config['uid'],config['gid'])!=(501,20):
        raise OriginalFailure('pinned image/ordinary user differs')
    if config.get('store_profile')!='Disposable/WAL/OFF' or not config.get('volume_clone_identity'):
        raise OriginalFailure('explicit Disposable independent installed Store clone required')
    deployment.load_plan(config,dict(arm='L',case_id='E01',cache_class='A'))
    for name in ('runtime','daemon'):
        if sha(config[name+'_binary'])!=config[name+'_sha256']:
            raise OriginalFailure('original sealed '+name+' binary changed')
    if sha(config['manifest'])!=config['manifest_sha256']:
        raise OriginalFailure('original installed cut manifest changed')
    output.mkdir()
    environment = output/'environment.txt'
    write_bytes(environment,''.join(key+'='+value+'\n' for key,value in sorted(workloads.ENV.items())).encode())
    first,fresh,scope,nonce = (secrets.token_hex(32) for _ in range(4))
    result = dict(schema=SCHEMA,status='INCOMPLETE',admission_eligible=False,performance_samples=0,
                  cache_treatments=0,setup_wall_stop_seconds=360,functional_wall_stop_seconds=100,
                  outer_wall_stop_seconds=460,command_and_verifier_wall_stop_seconds=9.5,
                  lifecycle_wall_stop_seconds=15,cleanup_wall_stop_seconds=5,phases=[],
                  image_id=IMAGE,volume=config['volume'],volume_clone_identity=config['volume_clone_identity'],
                  config_sha256=sha(config_path),controller_sha256=sha(__file__),environment=workloads.ENV,
                  default_policy_artifact=policy_artifact,default_policy=config.get('git_default_policy'),
                  global_profile='Disposable/WAL/OFF',overlay_profile='MEMORY/OFF/EXCLUSIVE',
                  durable='NOT_RUN — disabled by owner until explicit reauthorization',
                  proof_scope='actual selected Git-index decoding and exact L index-byte/metadata Commit/fresh-mount survival',
                  native_comparison='NOT_CLAIMED',generic_workload_oracle='NOT_CLAIMED',stat_cache_correctness='NOT_CLAIMED')
    write_json(output/'prospective.json',result)
    argv = [config['runtime_binary'],'serve','--socket',config['socket'],'--volume',config['volume'],
            '--daemon',config['daemon_binary'],'--manifest',config['manifest'],'--receipt',str(output/'runtime.events.jsonl'),
            '--uid','501','--gid','20','--environment-file',str(environment),'--observation-scope',scope]
    runtime = None
    try:
        runtime = EventProcess(argv,output,'runtime')
        runtime.event('protocol_ready',min(outer_deadline,started+360,time.monotonic()+15))
        if not runtime.container:
            raise OriginalFailure('original SDK container acknowledgement missing')
        result['staging'] = deployment.stage(config,dict(arm='L',case_id='E01',cache_class='A'),runtime.container,output/'setup')
        if time.monotonic()>=min(outer_deadline,started+360):
            raise TimeoutError('separate setup wall stop exceeded')
        deadline = min(outer_deadline,time.monotonic()+99)
        helper_sha = result['staging']['helper_sha256'].get('git_index_oracle.py')
        if helper_sha != sha(REPOSITORY/'core/benchmark/fs-bench-pro/r7/git_index_oracle.py'):
            raise OriginalFailure('actual staged index helper differs from selected host decoder')
        common = dict(inside='/tmp/layerfs-r7-git-index-proof-'+nonce,helper_sha256=helper_sha,
                      environment=workloads.ENV,known_defaults=DEFAULTS)
        attempt(runtime,output,result,'mount',first,None,15,deadline,2)
        query_script = body(output,'original-queries',dict(common,mode='query',default_policy=config.get('git_default_policy')))
        query_event = attempt(runtime,output,result,'command',first,query_script,9.5,deadline)
        pin,artifact = stdout_json(query_event,output)
        if pin.get('schema')!='r7-git-index-original-pin-v1' or pin.get('status')!='OBSERVED':
            raise OriginalFailure('original query/pin receipt missing')
        result['original_queries_artifact']=artifact
        result['actual_git_pin']=pin['pin']
        pin_sha = pin['pin_file_sha256']
        def snapshot(key,label,expected=None):
            began = time.monotonic_ns()
            proof_deadline = min(deadline, began / 1e9 + 9.5)
            record = dict(label=label,start_ns=began,wall_stop_ns=9500000000,status='ORIGINAL_PENDING')
            result.setdefault('verifier_attempts',[]).append(record)
            try:
                script = body(output,label,dict(common,mode='observe',pin_file_sha256=pin_sha))
                event = attempt(runtime,output,result,'verify',key,script,9.5,proof_deadline)
                record['original_event']=event
                observed,artifact = stdout_json(event,output)
                if observed.get('schema')!=git_index_oracle.SCHEMA or observed.get('status')!='OBSERVED' or observed.get('checksum',{}).get('verified') is not True:
                    raise OriginalFailure('actual selected index observation failed')
                result[label] = dict(artifact=artifact,raw_index_sha256=observed['raw_index_sha256'],
                                     raw_index_bytes=observed['raw_index_bytes'],semantic_entries=observed['semantic']['entry_count'],
                                     version=observed['semantic']['version'],filesystem_metadata=observed['filesystem_metadata'],checksum=observed['checksum'])
                if expected is not None:
                    result['survival_comparison']=compare_same_l_index(expected,observed)
                record['status']='OBSERVED'
                return observed
            except Exception as error:
                record.update(status='FAILED',original_error=str(error),original_error_type=type(error).__name__)
                raise
            finally:
                ended = time.monotonic_ns()
                record.update(end_ns=ended,duration_ns=ended-began)
                if record['status']=='OBSERVED' and (ended-began>9500000000 or time.monotonic()>=proof_deadline):
                    record.update(status='FAILED',original_error='complete independent index verifier wall stop exceeded',original_error_type='TimeoutError')
                    raise TimeoutError(record['original_error'])
        initial = snapshot(first,'initial-index')
        status_script = output/'original-status.sh'
        write_bytes(status_script,workloads.COMMANDS['E04'].encode())
        status_event = attempt(runtime,output,result,'command',first,status_script,9.5,deadline)
        result['original_status_stdout'] = dict(path=status_event['fields']['stdout'],sha256=sha(status_event['fields']['stdout']),
                                              scope='original output retained; no generic E04/native oracle verdict')
        before = snapshot(first,'before-Commit-index')
        result['index_changed_after_status']=initial['raw_index_sha256']!=before['raw_index_sha256']
        commit = attempt(runtime,output,result,'commit',first,None,15,deadline,1)
        if commit.get('fields',{}).get('commit_kind') not in ('Committed','UpToDate'):
            raise OriginalFailure('known original Commit outcome missing')
        if result['index_changed_after_status'] and commit['fields']['commit_kind']!='Committed':
            raise OriginalFailure('changed index bytes did not produce typed Committed')
        attempt(runtime,output,result,'unmount',first,None,15,deadline,1)
        cleanup(runtime,output,result,first,deadline)
        attempt(runtime,output,result,'mount',fresh,None,15,deadline,2)
        # No new Git command occurs before these raw/semantic reads.
        after = snapshot(fresh,'fresh-mount-index',before)
        attempt(runtime,output,result,'unmount',fresh,None,15,deadline,1)
        cleanup(runtime,output,result,fresh,deadline)
        attempt(runtime,output,result,'stop','',None,15,deadline)
        runtime.event('finished',min(deadline,time.monotonic()+5))
        runtime.process.stdin.close()
        code = runtime.process.wait(timeout=max(0.001,deadline-time.monotonic()))
        if code!=0:
            raise OriginalFailure('original runtime exit nonzero')
        result.update(status='PASS_FUNCTIONAL_ONLY',container_stop='KNOWN_STOP',known_host_exit=code,
                      EndSession='original runtime stop implements exact SessionEnded before acknowledged container stop',
                      fresh_mount_before_new_Git=True)
    except Exception as error:
        result.update(status='FAIL',original_failure_type=type(error).__name__,original_failure=str(error),
                      original_phase=getattr(error,'original_phase',None),
                      independent_close_failures=getattr(error,'independent_close_failures',[]))
        if runtime is None:
            runtime = getattr(error, 'event_process', None)
        if runtime is not None:
            try:
                result['retained_custody']=host_fence(runtime,output,error,result)
            except Exception as failure:
                result['independent_host_custody_failure']=dict(type=type(failure).__name__,failure=str(failure))
    result['complete_proof_ns']=int((time.monotonic()-started)*1e9)
    if runtime is not None:
        result.update(container=runtime.container,exec_ids=runtime.exec_ids,daemon_instance=runtime.daemon_instance,scope=runtime.scope)
    try:
        write_json(output/'result.json',result)
    except Exception as failure:
        result.update(status='FAIL',independent_result_output_failure=dict(type=type(failure).__name__,failure=str(failure)))
    print(json.dumps(result,sort_keys=True))
    return 0 if result['status']=='PASS_FUNCTIONAL_ONLY' else 1


if __name__=='__main__':
    raise SystemExit(main())
