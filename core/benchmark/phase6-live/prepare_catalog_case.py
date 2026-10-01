#!/usr/bin/env python3
"""Extend a closed original input with generic live enumeration/churn observations."""
from pathlib import Path
import argparse
import hashlib
import json
import struct

def read_case(path):
    raw=Path(path).read_bytes()
    if not raw.startswith(b'P6CASE1\0'):
        raise ValueError('source case framing')
    at=8
    def number():
        nonlocal at
        n=struct.unpack_from('>I',raw,at)[0];at+=4;return n
    def text():
        nonlocal at
        n=number();v=raw[at:at+n]
        if len(v)!=n:raise ValueError('source case EOF')
        at+=n;return v.decode()
    name=text();count=number()
    if count not in (3,5):raise ValueError('expected sealed three/five-step input')
    steps=[tuple(text() for _ in range(3)) for _ in range(count)]
    if at!=len(raw):raise ValueError('source case trailing bytes')
    return name,steps,hashlib.sha256(raw).hexdigest()

def encode(name,steps):
    out=bytearray(b'P6CASE1\0')
    def text(value):
        b=value.encode();out.extend(struct.pack('>I',len(b)));out.extend(b)
    text(name);out.extend(struct.pack('>I',len(steps)))
    for step in steps:
        for value in step:text(value)
    return bytes(out)

def main():
    p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--output',required=True);p.add_argument('--identity',required=True);args=p.parse_args()
    source,steps,digest=read_case(args.source)
    if source!='phase6-live-many128-v1' or len(steps)!=3:raise ValueError('original case identity/cardinality')
    manifest=steps[-1][2]
    churn='i=0; while [ "$i" -lt 64 ]; do printf edit-0 > many/f0 || exit; i=$((i+1)); done; test "$(cat many/f0)" = edit-0 && test "$(find many -maxdepth 1 -type f | wc -l)" -eq 128'
    # Preserve pipeline failure: a failed retirement/count test must not be hidden
    # by the last while loop's successful exit.
    clean='test -z "$(find /layerfs/backing -path \'*/sources/*\' -type f -print -quit)" || exit 1; test "$(find many -maxdepth 1 -type f | wc -l)" -eq 128 || exit 1; i=0; while [ "$i" -lt 128 ]; do test -f "many/f$i" || exit; i=$((i+1)); done'
    extra=[('enumeration-churn',churn,manifest),('clean-retirement',clean,manifest)]
    result=encode('phase6-live-catalog128-v1',steps+extra)
    out=Path(args.output)
    with out.open('xb') as f:f.write(result)
    with Path(args.identity).open('x') as f:json.dump({'source_case':source,'source_sha256':digest,'case_sha256':hashlib.sha256(result).hexdigest(),'original_steps_preserved':True,'steps':[(s[0],s[1],hashlib.sha256(s[2].encode()).hexdigest()) for s in steps+extra],'performance_limit_seconds':15,'proof_limit_seconds':9.5,'cache':'INELIGIBLE: OS/page cache unknown'},f,indent=2);f.write('\n')
    print(hashlib.sha256(result).hexdigest())

if __name__=='__main__':main()
