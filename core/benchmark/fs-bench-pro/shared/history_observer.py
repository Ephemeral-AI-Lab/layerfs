"""Sealed fixed-count SQLite/VFS observer; no product/dependency override."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
FLAGS = ['-O2','-std=c11','-Wall','-Wextra','-Werror','-dynamiclib']
NAMES = ['sqlite_history_observer.c','sqlite_work.c','sqlite_close_observer.c','sqlite_vfs_observer.c']


def build(root, out, invoke, common):
    sources = [root/'core/benchmark/fs-bench-pro/diagnostics'/name for name in NAMES]
    inputs = {'sources':{name:common.digest(p) for name,p in zip(NAMES,sources)},
              'flags':FLAGS,'compiler':subprocess.check_output(['/usr/bin/clang','--version'],text=True),
              'platform':platform.platform(),'architecture':platform.machine(),'sqlite_link':'unmodified system -lsqlite3'}
    seal = hashlib.sha256(json.dumps(inputs,sort_keys=True).encode()).hexdigest()
    folder = root/'benchmark-results/fs-bench-pro/history-observer-archive'/seal
    meta = folder/'identity.json';binary=folder/'history-observer.dylib'
    if meta.exists():
        prior=json.loads(meta.read_text())
        if prior['inputs']!=inputs or common.digest(binary)!=prior['sha256']:raise ValueError('history observer seal mismatch')
        return {**prior,'mode':'sealed-binary-reuse'}
    folder.mkdir(parents=True,exist_ok=False)
    result=invoke(['/usr/bin/clang',*FLAGS,str(sources[0]),'-lsqlite3','-o',str(binary)],out,'observer-build',30_000_000_000,os.environ.copy(),root)
    if result['exit_code'] or result['timed_out']:raise ValueError('history observer build failed; retained output')
    binary.chmod(0o555)
    record={'inputs':inputs,'seal':seal,'path':str(binary),'sha256':common.digest(binary),'build':result,'mode':'compiled',
            'coverage':'trace statements/VM plus delegated VFS read/write/sync/close; native VM reset counters unqualified; no device-byte/syscall claim'}
    meta.write_text(json.dumps(record,sort_keys=True,indent=2)+'\n');return record


def collect(out):
    sql=json.loads((out/'sql-work.json').read_text());vfs=json.loads((out/'vfs.json').read_text())
    if sql['opens']<=0 or sql['omitted'] or vfs['live_files'] or vfs['close_errors'] or vfs['unknown_sync_flags']:
        raise ValueError('history observer coverage incomplete')
    return {'status':'CHECKED','sql':sql,'vfs':vfs,'native_vm_scope':'UNQUALIFIED due trace reset; use observer VM counts',
            'cost_scope':'inclusive observer overhead in both compared lifecycles; nested call spans not additive'}
