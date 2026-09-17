#!/usr/bin/env python3
import subprocess, re, json, sys
REPO = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs"
sel = sys.argv[1:]
out = subprocess.run(["git","-C",REPO,"log","--reverse","--format=%h%x01%s%x01%B%x02","579831eb6..HEAD"],capture_output=True,text=True).stdout
for chunk in out.split("\x02"):
    chunk = chunk.strip("\n")
    if not chunk: continue
    parts = chunk.split("\x01")
    short, subj = parts[0], parts[1]
    body = "\x01".join(parts[2:])
    if sel and short not in sel: continue
    print("="*100)
    print(short, subj)
    for line in body.splitlines():
        if re.search(r"Production LOC|LOC|loc", line):
            print("   ", line)
