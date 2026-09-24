#!/usr/bin/env python3
"""Fresh symlink declarations through unchanged prepared native Service routes."""
import argparse
import copy
import fcntl
import json
import os
from pathlib import Path
import signal
import struct
import sys
import time

import prepared_files_route as driver
import construct_metadata_route as metadata_constructor
import construct_symlink_route as symlink_constructor
shared = driver.shared
WORKSPACE = b'\x7a' * 32
TARGET = b'../dangling/\xff'
CASES = {
    'symlinks': ['fresh-symlink-objects', 'direct-candidate', 'explicit-stage-and-CommitStaged',
                 'fresh-composite-Commit', 'immutable-roots-and-exact-history', 'cleanup'],
    'refusals': ['symlink-role-binding-and-parent-refusals', 'unchanged-namespace-stage-and-allocation', 'cleanup'],
}

class Peer(shared.Peer):
    def symlink(self, target):
        return symlink_constructor.saved(self.call(16, shared.route.blob(target)), len(target))['root']

    def readlink(self, root, path):
        r=shared.route.Reader(self.call(2, root+b'\x03'+shared.route.blob(path)))
        assert r.u8()==6
        value=r.blob();r.done();return value


def encode(value):
    if not value['symlinks']:
        return driver.encode(value)
    body=shared.encode_preparation(value | {'new': [], 'patches': []})
    body+=b'\x03'+struct.pack('>H',len(value['new']))
    body+=b''.join(struct.pack('>QIqI',*row) for row in value['new'])
    body+=struct.pack('>H',len(value['patches']))
    body+=b''.join(struct.pack('>QIqI',*row) for row in value['patches'])
    for key in ('files','symlinks'):
        body+=struct.pack('>H',len(value[key]))+b''.join(struct.pack('>Q',serial) for serial in value[key])
    return body


def captured(snapshot,value,command,generation=1):
    return (bytes((command,))+WORKSPACE+snapshot['branch']['branch']
            +shared.route.optional(snapshot['branch']['head_commit'])+snapshot['branch']['base_layer']
            +struct.pack('>Q',generation)+encode(value))


def preparation(snapshot,first,roots,metadata,prefix=b''):
    parent=int.from_bytes(snapshot['root_serial'],'big')
    names=(b'empty',b'file',b'self')
    p=shared.preparation(snapshot,[(parent,[(prefix+name,first+i) for i,name in enumerate(names)])],
        inodes=[{'serial':first+i,'kind':3,'content':root,'metadata':metadata} for i,root in enumerate(roots)])
    return p | {'files': [], 'symlinks': list(range(first,first+3))}


def saved(peer,value):
    r=shared.route.Reader(peer.call(5,encode(value)));assert r.u8()==7
    result={'root':r.take(32),'inserted':r.u64(),'reused':r.u64()};r.done();return result


def verify(peer,root,first,roots,metadata,prefix=b''):
    result={}
    for i,(name,target) in enumerate([(b'empty',b''),(b'file',TARGET),(b'self',prefix+b'self')]):
        path=prefix+name;value=peer.attributes(root,path)
        assert value=={'serial':first+i,'kind':3,'references':1,'content':roots[i],
            'metadata':metadata,'mode':0o777,'mtime':-2,'nanoseconds':17,'size':len(target)}
        assert peer.readlink(root,path)==target
        result[path.decode()]=value
    return shared.printable(result)


def exercise(peer,snapshot,case,report):
    branch,old=snapshot['branch']['branch'],snapshot['effective_root']
    old_attrs={name.decode():peer.attributes(old,name) for name in (b'',b'data.bin',b'alias')}
    old_listing,old_history=peer.listing(old,b''),peer.commits(branch)
    count=9 if case=='symlinks' else 3
    reservation=peer.reserve(snapshot['scope'],count);first=reservation['start']
    report['reservation']=shared.printable(reservation)
    roots=[peer.symlink(target) for target in (b'',TARGET,b'self')]
    next_roots=roots[:2]+[peer.symlink(b'next-self')] if case=='symlinks' else None
    metadata=metadata_constructor.constructed(peer.call(15,metadata_constructor.payload(3,0o777,-2,17)))['metadata']
    assert peer.branch(branch)==snapshot and peer.commits(branch)==old_history
    if case=='symlinks':
        with shared.check(report,'fresh-symlink-objects') as row:
            row.update(roots=[root.hex() for root in roots],targets=shared.printable([b'',TARGET,b'self']),
                metadata=metadata.hex(),standalone_constructors=4,metadata_constructors=1,
                next_self_root=next_roots[2].hex(),next_self_target='next-self')
        with shared.check(report,'direct-candidate') as row:
            direct=saved(peer,preparation(snapshot,first,roots,metadata))
            row.update(candidate=shared.printable(direct),attributes=verify(peer,direct['root'],first,roots,metadata))
            assert peer.branch(branch)==snapshot and peer.commits(branch)==old_history
            peer.call(6,b'\x09'+WORKSPACE,2,failure=14)
        with shared.check(report,'explicit-stage-and-CommitStaged') as row:
            tag,stage=peer.command(captured(snapshot,preparation(snapshot,first+3,roots,metadata),3))
            assert tag=='Stage' and stage['candidate_root']!=direct['root']
            row['attributes']=verify(peer,stage['candidate_root'],first+3,roots,metadata)
            assert peer.branch(branch)==snapshot and peer.commits(branch)==old_history
            tag,one=peer.command(b'\x04'+WORKSPACE+struct.pack('>Q',stage['token']))
            assert tag=='Committed' and one['root']==stage['candidate_root'] and one['parent']==snapshot['branch']['head_commit']
            peer.call(6,b'\x09'+WORKSPACE,2,failure=14)
            assert peer.commits(branch)==[one]+old_history
            row.update(stage=shared.printable(stage),commit=shared.printable(one))
        with shared.check(report,'fresh-composite-Commit') as row:
            current=peer.branch(branch);assert current['effective_root']==one['root']
            tag,two=peer.command(captured(current,preparation(current,first+6,next_roots,metadata,b'next-'),5,generation=2))
            assert tag=='Committed' and two['parent']==one['commit'] and peer.branch(branch)['effective_root']==two['root']
            row.update(commit=shared.printable(two),attributes=verify(peer,two['root'],first+6,next_roots,metadata,b'next-'))
        with shared.check(report,'immutable-roots-and-exact-history') as row:
            verify(peer,direct['root'],first,roots,metadata)
            verify(peer,one['root'],first+3,roots,metadata)
            verify(peer,two['root'],first+3,roots,metadata)
            for root in (old,direct['root'],one['root'],two['root']):
                for name in (b'data.bin',b'alias'):assert peer.attributes(root,name)==old_attrs[name.decode()]
            assert peer.listing(old,b'')==old_listing and peer.commits(branch)==[two,one]+old_history
            peer.call(2,old+b'\x04'+shared.route.blob(b'empty'),failure=7)
            row.update(old_root_unchanged=True,explicit_CommitStaged_calls=1,explicit_CompositeCommit_calls=1,
                workload_history_records_added=2,declaration_reexposure=False,symlink_references=1,
                target_resolution='none: self target matches final basename in all routes; dangling opaque targets retained literally')
    else:
        baseline=preparation(snapshot,first,roots,metadata)
        no_change=shared.preparation(snapshot) | {'files': [], 'symlinks': []}
        tag,previous_stage=peer.command(captured(snapshot,no_change,3));assert tag=='Stage'
        cases=[]
        value=copy.deepcopy(baseline);value['symlinks'][0]=0;cases.append(('zero',value,1,True))
        value=copy.deepcopy(baseline);value['symlinks'].append(first+3);cases.append(('S-exceeds-I-F',value,4,True))
        value=copy.deepcopy(baseline);value['inodes'][0]['kind']=1;cases.append(('wrong-kind',value,1,True))
        value=copy.deepcopy(baseline);existing=old_attrs['data.bin']['serial']
        value['symlinks'][0]=existing;value['inodes'][0]['serial']=existing;value['directories'][0][1][0]=(b'empty',existing)
        cases.append(('existing-base-serial',value,1,False))
        for name,key,root,code in [('wrong-content-role','content',old_attrs['']['content'],1),
            ('wrong-metadata-role','metadata',roots[1],1),('missing-content','content',b'\xee'*32,6),
            ('missing-metadata','metadata',b'\xee'*32,6)]:
            value=copy.deepcopy(baseline);value['inodes'][0][key]=root;cases.append((name,value,code,False))
        value=copy.deepcopy(baseline);value['directories'].clear();cases.append(('unbound-symlink',value,1,False))
        value=copy.deepcopy(baseline);value['symlinks'].clear();cases.append(('undeclared-fresh',value,1,False))
        value=copy.deepcopy(baseline);value['directories'][0][1].append((b'zalias',first));cases.append(('symlink-hardlink',value,1,False))
        value=copy.deepcopy(baseline);value['directories'].append((first,[(b'child',first+1)]));cases.append(('symlink-parent',value,1,False))
        value=copy.deepcopy(baseline);value['scope']=b'\xee'*32;cases.append(('scope',value,1,False))
        with shared.check(report,'symlink-role-binding-and-parent-refusals') as row:
            row['attempts']=[]
            for name,value,code,local in cases:
                for label,opcode,profile,body in [('direct',5,1,encode(value)),
                    ('stage',7,2,captured(snapshot,value,3)),('commit',7,2,captured(snapshot,value,5))]:
                    failure=peer.call(opcode,body,profile,failure=code,allow_local=local)
                    assert peer.branch(branch)==snapshot and peer.commits(branch)==old_history
                    assert peer.query(b'\x09'+WORKSPACE)==('Stage',previous_stage)
                    row['attempts'].append({'case':name,'route':label,**failure})
        with shared.check(report,'unchanged-namespace-stage-and-allocation') as row:
            assert peer.listing(old,b'')==old_listing
            for name,attributes in old_attrs.items():assert peer.attributes(old,name.encode())==attributes
            tag,removed=peer.command(b'\x07'+WORKSPACE+struct.pack('>Q',previous_stage['token']))
            assert tag=='Discarded' and removed==1
            row.update(old_root_unchanged=True,prior_stage_preserved_then_explicitly_discarded=True,
                Commit_records_added=0,unbound_or_hardlinked_symlink='InvalidInput; no directory-style omission')
    after=peer.reserve(snapshot['scope'],1);assert after['start']==first+count
    report['subsequent_reservation']=shared.printable(after)
    report['allocation_scope']='explicit live C5 reservation; prepared operations consumed no additional range'


driver.Peer=Peer
driver.exercise=exercise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('fixture', 'binaries', 'output'): parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True); args = parser.parse_args()
    for name in ('fixture', 'binaries', 'output'): setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False); shared.route.BIN = args.binaries
    os.environ['LAYERFS_CONSTRUCTION_WORKERS'] = '1'
    report = {'status': 'FAIL', 'mode': 'functional-native-prepared-fresh-symlinks', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CASES[args.case]],
        'hard_budget_seconds': 60, 'request_budget_ms': 5000, 'construction_workers': 1,
        'performance_claim': False, 'cache_claim': None, 'container_cpu_quota': 'not applicable: host Service/client',
        'not_run': ['Workspace/kernel SYMLINK', 'prepared npm workload', 'R6/RSS/performance',
                    'reservation ownership provenance enforcement', 'unknown prepared save recovery']}
    started = time.monotonic()
    def expired(_signal, _frame): raise TimeoutError('complete fresh-symlink selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = shared.stage_route.isolation.namespace()
        for path in (args.fixture, args.binaries, args.output): space.assert_owned(path, 'fresh-symlink proof input/output')
        report.update(source=shared.mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
            product_inputs_sha256=shared.mount.product_inputs(), driver_sha256=shared.sha(Path(__file__)),
            binaries={name: shared.sha(args.binaries / name) for name in ('layerfs-server', 'layerfs-daemon', 'examples/public_key')},
            resource_isolation=space.as_fields(),
            caller_dependencies_sha256={str(Path(module.__file__).resolve().relative_to(shared.ROOT)): shared.sha(Path(module.__file__))
                for module in tuple(sys.modules.values()) if getattr(module, '__file__', None)
                and Path(module.__file__).resolve().is_relative_to(shared.ROOT) and Path(module.__file__).suffix == '.py'})
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB); driver.execute(args, report)
        report['status'] = 'PASS'
    except BaseException as error: report['failure'] = repr(error); raise
    finally:
        signal.alarm(0); report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
