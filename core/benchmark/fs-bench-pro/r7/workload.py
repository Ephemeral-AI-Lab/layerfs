"""Fixed one-worker workloads; progress acknowledgements survive the 60-second stop."""
import argparse
import json
import os
import shutil
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('task',choices=['append','churn','copy','link','remove']);a=p.parse_args()
progress=Path('/tmp/experiment-progress.json')
n=0;b=0

def tick():
    progress.write_text(json.dumps({'entries_completed':n,'bytes_written':b}))

if a.task=='append':
    with open('experiment.log','wb',buffering=0) as f:
        for i in range(10000):
            f.write(b'x'*99+b'\n'); n+=1;b+=100
            if n%100==0:tick()
elif a.task=='churn':
    for i in range(10000):
        p=Path(f'experiment-temp-{i}')
        p.write_bytes(b'x');p.unlink();n+=1;b+=1
        if n%100==0:tick()
elif a.task=='remove':
    roots=json.loads(Path('/code/node-roots.json').read_text())
    os.execvp('rm',['rm','-rf','--',*roots])
else:
    rows=json.loads(Path('/replay/master.json').read_text())
    selected=[(p,r) for p,r in rows.items() if 'node_modules' in Path(p).parts]
    store=Path('.experiment-store')
    if a.task=='link':store.mkdir()
    for rel,row in sorted(selected,key=lambda pr:(len(Path(pr[0]).parts),pr[0])):
        dst=Path(rel);src=Path('/replay/F')/rel
        if row['kind']=='directory':dst.mkdir(exist_ok=True);os.chmod(dst,row['mode'])
        elif row['kind']=='symlink':dst.symlink_to(row['target'])
        else:
            if a.task=='copy':shutil.copyfile(src,dst)
            else:
                s=store/str(n);shutil.copyfile(src,s);os.link(s,dst)
            os.chmod(dst,row['mode']);b+=row['size']
        n+=1
        if n%100==0:tick()
tick()
print(json.dumps({'entries_completed':n,'bytes_written':b},sort_keys=True))
