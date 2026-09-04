#!/usr/bin/env python3
"""Serialize source-bound diagnostic commands and retain exact stdout/stderr."""
import hashlib, json, pathlib, subprocess, sys, time
root = pathlib.Path(__file__).resolve().parents[2]
label, *command = sys.argv[1:]
out = root / 'investigations/workspace-commit/evidence' / label
out.mkdir()
source = subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
start=time.monotonic_ns()
with (out/'stdout').open('xb') as stdout, (out/'stderr').open('xb') as stderr:
    result=subprocess.run(command,cwd=root,stdout=stdout,stderr=stderr)
receipt={'source':source,'command':command,'elapsed_ns':time.monotonic_ns()-start,'exit_code':result.returncode}
for name in ['stdout','stderr']:
    receipt[name+'_sha256']=hashlib.sha256((out/name).read_bytes()).hexdigest()
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
sys.exit(result.returncode)
