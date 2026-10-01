#!/usr/bin/env python3
"""Reuse closed original cases, append an ordinary command trust-boundary proof."""
import argparse,hashlib,json
from pathlib import Path
from prepare_catalog_case import read_case,encode
COMMAND=r'''test "$(id -u)" -eq 65534 || exit 1; test "$(id -g)" -eq 65534 || exit 1; test -z "${LAYERFS_PRIVATE_KEY+x}${LAYERFS_ENDPOINT+x}${LAYERFS_CONTROL_PEERS+x}${MINIO_ROOT_PASSWORD+x}" || exit 1; if cat /proc/1/environ >/dev/null 2>&1; then exit 1; fi; if ls /layerfs/backing >/dev/null 2>&1; then exit 1; fi; if kill -0 1 2>/dev/null; then exit 1; fi; awk '/^(CapInh|CapPrm|CapEff|CapAmb):/ {if ($2 != "0000000000000000") bad=1; n++} /^NoNewPrivs:/ {if ($2 != "1") bad=1; p++} END {if (bad || n != 4 || p != 1) exit 1}' /proc/self/status'''
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--output',required=True);p.add_argument('--identity',required=True);a=p.parse_args()
 name,steps,digest=read_case(a.source)
 if name not in ('phase6-live-many128-v1','phase6-live-components270-v1') or len(steps)!=3:raise ValueError('original identity/cardinality')
 result=encode(name.replace('-v1','-trust-v1'),steps+[('publisher-boundary',COMMAND,steps[-1][2])]);out=Path(a.output)
 with out.open('xb')as f:f.write(result)
 record={'source_case':name,'source_sha256':digest,'case_sha256':hashlib.sha256(result).hexdigest(),'original_steps_preserved':True,'original_step_seals':[{'name':s[0],'command_sha256':hashlib.sha256(s[1].encode()).hexdigest(),'manifest_sha256':hashlib.sha256(s[2].encode()).hexdigest()}for s in steps],'new_command':COMMAND,'new_manifest_sha256':hashlib.sha256(steps[-1][2].encode()).hexdigest(),'performance_limit_seconds':15,'proof_limit_seconds':9.5,'cache':'INELIGIBLE OS/page cache unknown'}
 with Path(a.identity).open('x')as f:json.dump(record,f,indent=2);f.write('\n')
 print(record['case_sha256'])
if __name__=='__main__':main()
