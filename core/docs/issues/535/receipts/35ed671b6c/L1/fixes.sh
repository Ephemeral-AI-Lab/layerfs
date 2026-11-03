#!/bin/bash
# L1: the "fixed since fe6f85013b" items reachable on macOS native/docker,
# through the CLI (and the Python package where noted), in home <H> with
# docker running.   fixes.sh <home> <data folder> <trace dir>
# Env: the lane env (LFS, K, LAYERFS_BUNDLE, LAYERFS_DAEMON, LAYERFS_PG_L1).
H=$1; DATA=$2; T=$3; mkdir -p "$T"
l() { LAYERFS_HOME=$H "$LFS" "$@"; }
run() { echo "\$ layerfs $*"; l "$@" 2>&1 | head -c 900; echo " (exit ${PIPESTATUS[0]})"; }
trun() { # $1 trace file, rest: the call
  local f=$1; shift; echo "\$ LAYERFS_TRACE=<$(basename $f)> layerfs $*"; LAYERFS_TRACE=$f LAYERFS_HOME=$H "$LFS" "$@" 2>&1 | head -c 400; echo " (exit ${PIPESTATUS[0]}); trace lines: $(cat $f 2>/dev/null | wc -l | tr -d ' ')"; cat "$f" 2>/dev/null | sed 's/^/   trace: /'
}
echo "# F-a: CLI store push after a CLI branch fork (K2 D3; the CLI closes its Stores)"
run store create f1; run project init f1/p "$DATA"; run branch fork f1/p lane main
run store status f1
run store push f1 docker
run store pull f1
run store remove f1

echo; echo "# F-b: LAYERFS_TRACE worker.connection lines, one per worker-routed Store call (item 6 R1)"
run store create t1; run project init t1/p "$DATA"; run branch fork t1/p lane main; run store push t1 docker
trun $T/t-file-list file list t1/p lane /
trun $T/t-branch-list branch list t1/p
trun $T/t-file-read file read t1/p main /a.txt
trun $T/t-three-calls file list t1/p main /
trun $T/t-three-calls branch list t1/p
trun $T/t-three-calls file read t1/p lane /a.txt
echo "## a native Store call writes no line"
run store create t2; run project init t2/p "$DATA"
trun $T/t-native file list t2/p main /
echo "## the same through the Python package: three Store calls on t1 in one process"
LAYERFS_TRACE=$T/t-python LAYERFS_HOME=$H PYTHONPATH=$K/python/src python3 -c "
import layerfs, os
c = layerfs.client(home=os.environ['LAYERFS_HOME'])
c.file.list('t1/p', 'main', '/'); c.branch.list('t1/p'); c.file.read('t1/p', 'main', '/a.txt')
print('python: three calls answered')"
echo "trace lines: $(wc -l < $T/t-python | tr -d ' ')"; sed 's/^/   trace: /' $T/t-python
echo "## a docker Sandbox's exec (recorded spec difference R2: exec goes through the worker)"
run sandbox create t1/p st1 docker
W=$(l workspace mount st1 lane 2>&1 | python3 -c "import json,sys;print(json.load(sys.stdin)['ws'])")
echo "mounted $W"
trun $T/t-exec workspace exec "$W" "true"
run workspace unmount "$W" --discard
run sandbox close st1
run store pull t1; run store remove t1; run store remove t2

echo; echo "# F-c: workspace.mount of main refused InvalidInput (owner D14), native and docker"
run store create m1; run project init m1/p "$DATA"; run branch fork m1/p lane main
run sandbox create m1/p sm1 native
run workspace mount sm1 main
run sandbox close sm1
run store push m1 docker
run sandbox create m1/p sm2 docker
run workspace mount sm2 main
run sandbox close sm2
run store pull m1; run store remove m1

echo; echo "# F-d: a missing PostgreSQL database"
run store connect gm postgres://postgres@127.0.0.1:55551/l1_no_such_db --password env:LAYERFS_PG_L1
run project list gm
run project init gm/p "$DATA"
run store status gm
run store disconnect gm

echo; echo "# F-e: store.status / store.list change nothing (stale lease of a killed process; no lock file, no lease pruning, no log line)"
run store create r1; run project init r1/p "$DATA"
LAYERFS_HOME=$H PYTHONPATH=$K/python/src python3 -c "
import layerfs, os, signal
c = layerfs.client(home=os.environ['LAYERFS_HOME']); c.file.list('r1/p', 'main', '/'); print('holder pid', os.getpid(), flush=True); os.kill(os.getpid(), signal.SIGKILL)"
hashes() { (cd "$H" && find . ! -path './trace/*' \( -type f -o -type d \) -print | LC_ALL=C sort | while read f; do if [ -f "$f" ]; then echo "$(shasum -a 256 "$f" | cut -c1-16) $f"; else echo "dir $f"; fi; done); }
echo "leases: $(ls $H/store/r1/open/ 2>&1 | tr '\n' ' ')  log/leases.jsonl lines: $(wc -l < $H/log/leases.jsonl 2>/dev/null || echo none)"
hashes > $T/before
run store status r1; run store list
LAYERFS_HOME=$H PYTHONPATH=$K/python/src python3 -c "
import layerfs, os, json
c = layerfs.client(home=os.environ['LAYERFS_HOME']); s = c.store.status('r1'); c.store.list(); print('python store.status state', s['state'], 'holders', json.dumps(s.get('holders')))"
hashes > $T/after
echo "leases: $(ls $H/store/r1/open/ 2>&1 | tr '\n' ' ')  log/leases.jsonl lines: $(wc -l < $H/log/leases.jsonl 2>/dev/null || echo none)"
if cmp -s $T/before $T/after; then echo "HOME UNCHANGED by store.status/store.list (CLI and Python)"; else echo "HOME CHANGED:"; diff $T/before $T/after; fi
run doctor --text
run store remove r1
echo FIXES-DONE
