#!/bin/bash
# K2 D2 recheck (fixed by 2d607fc066), lane L2: a PostgreSQL Store name
# reused with another database within the worker's 60 s idle time, one CLI
# process per call, Sandboxes on microsandbox machine lfs535l2m. Databases
# i3pg5 then i3pg6; the second time the Branch is lane2, and a Commit on it
# is read back from the caller. Copied from K2's item3/diag-stale-name.log steps.
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l2/env.sh
log=$R/item3/stale-name.log; m=lfs535l2m
L="postgres://postgres@127.0.0.1:$PGPORT_L2"
src=$(mktemp -d $S/src.XXXX); echo one > $src/a.txt
r() { $S/run.sh $log "$@"; }
ws() { python3 -c 'import json,sys;print(json.load(sys.stdin)["ws"])'; }
echo "# D2 recheck: name e1 -> database i3pg5, then e1 -> i3pg6 within 60 s" >> $log
r store connect e1 $L/i3pg5 --password env:LFSPW >/dev/null
r project init e1/p $src >/dev/null
r branch fork e1/p lane main >/dev/null
r sandbox create e1/p e1 $m >/dev/null
w=$(r workspace mount e1 lane | head -1 | ws)
r workspace unmount $w --discard >/dev/null
r sandbox close e1 >/dev/null
r store disconnect e1 >/dev/null
r store connect e1 $L/i3pg6 --password env:LFSPW >/dev/null
r project init e1/p $src >/dev/null
r branch fork e1/p lane2 main >/dev/null
r sandbox create e1/p e1 $m >/dev/null
w=$(r workspace mount e1 lane2 | head -1 | ws)
r workspace exec $w 'echo committed into i3pg6 > d2.txt' >/dev/null
r workspace commit $w --message d2 >/dev/null
r workspace unmount $w >/dev/null
r file read e1/p lane2 /d2.txt >/dev/null
r workspace mount e1 lane   # the old database's Branch must not exist here
r sandbox close e1 --force >/dev/null
r store disconnect e1 >/dev/null
rm -rf $src
