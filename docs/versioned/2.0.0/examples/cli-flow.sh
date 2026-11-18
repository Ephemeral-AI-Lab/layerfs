#!/usr/bin/env bash
# LayerFS 2.0.0 from the shell: the mountless flow with the `layerfs` command
# alone. Store, project, files, Branches, diff, merge, publish, history and
# refusals. No Sandbox, no Workspace, no machine.
#
# How it was run for this manual (see README.md in this folder for the
# output): with the debug build of the worktree on macOS,
#   LAYERFS=target/debug/layerfs TMPDIR=<scratch dir> bash cli-flow.sh
# It makes its own Disposable home and project folder in a temporary directory
# and removes them at the end. On Linux the temporary directory must be on a
# filesystem with SQLite locks (ext4, xfs, btrfs). Exit 0 is done, 2 a wrong
# command line, 125 a refusal.
set -u
LAYERFS=${LAYERFS:-layerfs}
root=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-guide-XXXXXX")
chmod 700 "$root"
trap 'rm -rf "$root"' EXIT
export LAYERFS_HOME="$root/home"
unset LAYERFS_NATIVE

# the project is a copy of a host folder
mkdir -p "$root/site/docs"
printf '# site\n' >"$root/site/README.md"
printf 'one\ntwo\n' >"$root/site/docs/a.txt"

P=shop/site
# run <layerfs words>: print the command, its output (stdout then stderr) and its exit code
run() {
  printf '$ layerfs'
  for word in "$@"; do
    case $word in *[[:space:]]*) printf " '%s'" "$word" ;; *) printf ' %s' "$word" ;; esac
  done
  echo
  "$LAYERFS" "$@" 2>&1
  echo "[exit $?]"
}

run store create shop
run project init $P "$root/site" --progress
run branch fork $P agent main
printf '# notes\n' | run file write $P agent /notes.md -m "add notes"
run file read $P agent /notes.md --text
run file list $P agent / --text
run file edit $P agent /docs/a.txt --old-string two --new-string three -m "edit a"
run file read $P agent /docs/a.txt --text

run branch fork $P left main
run branch fork $P right main
run file edit $P left /docs/a.txt --old-string one --new-string uno -m "left edits a"
printf 'from right\n' | run file write $P right /right.txt -m "right adds a file"
run branch merge $P left right --into merged -m "merge left and right"
run commit diff $P merged --from main --text
run file read $P merged /docs/a.txt --text
run commit list $P merged --text
run branch list $P --text
run branch publish $P merged
run commit list $P main

# refusals: exit 125 with {error, message, fix?} on stderr; exit 2 for a wrong command line
run file read $P nope /README.md
printf 'x\n' | run file write $P main /x.txt
run file read $P agent /README.md --colour
run file read $P agent
run file list $P agent /docs --scope /docs --scope-project $P --scope-branch ns
run store list --scope /docs --scope-project $P --scope-branch ns
