#!/bin/bash
# usage: seqx.sh <machine> <store> <prefix> <progress log>; C01..C12 one after another
X=$(dirname "$(readlink -f "$0")")
for n in 01 02 03 04 05 06 07 08 09 10 11 12; do
  echo "start C$n $(date -u +%T)" >> "$4"
  "$X/cellx.sh" C$n "$1" "$2" "$3" >> "$4" 2>&1
done
echo "DONE $1 $2 $(date -u +%T)" >> "$4"
