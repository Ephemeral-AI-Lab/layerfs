#!/bin/sh
# Runs TLC on each named config (default: every *.cfg here), each under a
# 120 s wall timeout. A config's module is its name up to the first "_"
# (adm_* -> Admission.tla; commit_*, ids_* -> Commit.tla). Logs go to logs/.
set -u
here=$(cd "$(dirname "$0")" && pwd)
jar=${TLA2TOOLS:-$HOME/.cache/layerfs-811/tools/tla2tools.jar}
to=${TO:-$HOME/.cache/layerfs-811/tools/to}
mkdir -p "$here/logs"
cd "$here" || exit 2
[ $# -gt 0 ] || set -- *.cfg
# Configs that must produce a counterexample (TLC exit 12 invariant, 13
# temporal property): the shipped variant B's findings, the rejected fixes,
# the mutations and the reachability probe.
expected=" adm_A_orphans adm_B_sep adm_B_one adm_B_rm adm_B_excl adm_B_F1late
 adm_B_F2 adm_B_live adm_B_F1_live adm_B_reopen1 adm_B_F1_reopen1
 adm_B_F1_reach commit_mut_nobarrier commit_mut_nolanecheck commit_mut_noend
 commit_mut_nofence ids_mut_nocas "
status=0
for cfg in "$@"; do
  cfg=${cfg%.cfg}
  case $cfg in
    adm_*) spec=Admission ;;
    commit_*|ids_*) spec=Commit ;;
    *) echo "unknown config $cfg"; exit 2 ;;
  esac
  start=$(date +%s)
  "$to" 120 java -XX:+UseParallelGC -cp "$jar" tlc2.TLC -workers auto \
      -deadlock -difftrace -cleanup -metadir "$here/logs/states-$cfg" \
      -config "$cfg.cfg" "$spec.tla" > "logs/$cfg.log" 2>&1
  rc=$?
  case "$expected" in
    *" $cfg "*) want=violation; { [ $rc -eq 12 ] || [ $rc -eq 13 ]; } && ok=1 || ok=0 ;;
    *) want=clean; [ $rc -eq 0 ] && ok=1 || ok=0 ;;
  esac
  verdict=$([ $ok -eq 1 ] && echo "as expected ($want)" || echo "UNEXPECTED (wanted $want)")
  echo "$cfg: exit $rc, $(( $(date +%s) - start )) s, $verdict (logs/$cfg.log)"
  rm -rf "$here/logs/states-$cfg"
  [ $ok -eq 1 ] || status=1
done
exit $status
