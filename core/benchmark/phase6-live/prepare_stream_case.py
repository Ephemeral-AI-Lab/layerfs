#!/usr/bin/env python3
"""Preserve a closed catalog fixture and append an ordinary sparse-write command."""
from pathlib import Path
import argparse,hashlib,json
from prepare_catalog_case import read_case,encode

def main():
    p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--output',required=True);p.add_argument('--identity',required=True);args=p.parse_args()
    source,steps,digest=read_case(args.source)
    if source!='phase6-live-catalog128-v1' or len(steps)!=5:raise ValueError('catalog case identity/cardinality')
    expected=bytearray(63*4096+1)
    for i in range(64):expected[i*4096]=ord('X')
    lines=steps[-1][2].rstrip().splitlines()
    if any(s.split('\t')[0]=='spans' for s in lines):raise ValueError('fixture already owns spans')
    lines.append('spans\tf\t420\t'+str(len(expected))+'\t'+hashlib.sha256(expected).hexdigest())
    manifest='\n'.join(sorted(lines))+'\n'
    command='i=0; while [ "$i" -lt 64 ]; do printf X | dd of=spans bs=1 seek=$((i*4096)) conv=notrunc 2>/dev/null || exit; i=$((i+1)); done; test "$(wc -c < spans)" -eq 258049 && test "$(dd if=spans bs=1 skip=258048 count=1 2>/dev/null)" = X'
    steps.append(('sparse-span-read',command,manifest));out=encode('phase6-live-stream128-v1',steps)
    with Path(args.output).open('xb') as f:f.write(out)
    with Path(args.identity).open('x') as f:json.dump({'source_case':source,'source_sha256':digest,'case_sha256':hashlib.sha256(out).hexdigest(),'original_steps_preserved':5,'new_command':command,'literal_file_bytes':len(expected),'literal_file_sha256':hashlib.sha256(expected).hexdigest(),'manifest_sha256':hashlib.sha256(manifest.encode()).hexdigest(),'performance_limit_seconds':15,'proof_limit_seconds':9.5,'cache':'INELIGIBLE: OS/page cache unknown'},f,indent=2);f.write('\n')
    print(hashlib.sha256(out).hexdigest())
if __name__=='__main__':main()
