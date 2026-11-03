#!/bin/bash
# runs C01..C12 one after another (one cell per command) and logs a line each
# usage: seq.sh <machine> <platform> <store> <prefix> <progress log>
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k3
for n in 01 02 03 04 05 06 07 08 09 10 11 12; do
  echo "start C$n $(date -u +%T)" >> "$5"
  $S/cell.sh C$n "$1" "$2" "$3" "$4" >> "$5" 2>&1
done
echo "DONE $2 $3 $(date -u +%T)" >> "$5"
