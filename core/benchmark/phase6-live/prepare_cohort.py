#!/usr/bin/env python3
"""Acquire named case inputs once from owning recipes; no runtime shortcut."""
import argparse, hashlib, importlib, json, struct, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro'))

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def blob(value):
    value=value.encode();return struct.pack('>I',len(value))+value

def main():
    p=argparse.ArgumentParser();p.add_argument('--output',required=True);a=p.parse_args()
    out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=False)
    namespace=importlib.import_module('families.workspace_namespace')
    package=importlib.import_module('families.workspace_shell_package')
    shell=importlib.import_module('shell_package')
    capture=ROOT/'core/docs/architecture/proposal/phase6-sqlite-minio/experiments/source-captures/r1-six-case-cohort.json'
    rows={r['label']:r for r in json.loads(capture.read_text())['original_spec']['rows']}
    cases=[]
    for label,case in [('namespace67',namespace.SDK['workspace-namespace-move-replace-descendants-67-sdk-v1']),('components270',namespace.SDK['workspace-namespace-components-270-sdk-v1'])]:
        old=namespace.tree(case.layout);old['.marker']=b'baseline';before,after=namespace.oracle(case)
        if label=='namespace67':successor='printf grand-next > packages/new/subtree/child/grand.txt';path='packages/new/subtree/child/grand.txt';data=b'grand-next'
        else:successor='i=0; while [ $i -lt 270 ]; do exec 3<. || exit; cd /proc/self/fd/3/d || exit; i=$((i+1)); done; printf leaf-next > file';path='/'.join(['d']*270)+'/file';data=b'leaf-next'
        cases.append((label,case,old,before,after,successor,path,data))
    case=package.Case('workspace-shell-package-many-128-sdk-v2','package',package.many_command(128),count=128)
    old,new=package.files(case);before=shell.manifest(old);after=shell.manifest(new)
    cases.append(('many128',case,old,before,after,'printf edit-0 > many/f0','many/f0',b'edit-0'))
    records=[]
    for label,case,tree,before,after,successor,path,data in cases:
        assert case.command==rows[label]['command'],label
        fixture=out/'fixtures'/label;fixture.mkdir(parents=True)
        for name,value in tree.items():
            if not name:continue
            dest=fixture/name
            if value is None:dest.mkdir(parents=True,exist_ok=True)
            else:dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(value);dest.chmod(0o644)
        modified=[]
        for line in after.splitlines():
            parts=line.split('\t')
            if parts[0]==path:parts[3]=str(len(data));parts[4]=hashlib.sha256(data).hexdigest()
            modified.append('\t'.join(parts))
        final='\n'.join(modified)+'\n'
        assert sum(l.split('\t')[0]==path for l in modified)==1
        steps=[('seed',f'umask 022; cp -R /fixtures/phase6/{label}/. .',before),('target',case.command,after),('localized-successor',successor,final)]
        case_id=f'phase6-live-{label}-v1';encoded=b'P6CASE1\0'+blob(case_id)+struct.pack('>I',len(steps))
        for name,command,manifest in steps:encoded+=blob(name)+blob(command)+blob(manifest)
        binary=out/(label+'.case');binary.write_bytes(encoded)
        for name,command,manifest in steps:(out/(label+'-'+name+'.manifest')).write_text(manifest)
        hashes={str(f.relative_to(fixture)):digest(f) for f in sorted(fixture.rglob('*')) if f.is_file()}
        records.append({'id':case_id,'original_id':case.id,'label':label,'case_file':str(binary),'case_sha256':digest(binary),'original_command_sha256':hashlib.sha256(case.command.encode()).hexdigest(),'fixture_files':hashes,'steps':[{'name':n,'command_sha256':hashlib.sha256(c.encode()).hexdigest(),'manifest_sha256':hashlib.sha256(m.encode()).hexdigest()} for n,c,m in steps]})
    sources=[Path(namespace.__file__),Path(package.__file__),Path(shell.__file__),capture,Path(__file__)]
    (out/'acquisition.json').write_text(json.dumps({'schema':'phase6-live-cohort-input-v1','sources':{str(f.relative_to(ROOT)):digest(f) for f in sources},'cases':records,'acquisition':'once from owning recipes; closed immutable Docker inputs; generic cp creates independent writable FUSE copy inside complete child','cache':'INELIGIBLE unknown; no warm/cold equivalence claim'},indent=2)+'\n')
    print(json.dumps({'output':str(out),'cases':[r['id'] for r in records]}))
if __name__=='__main__':main()
