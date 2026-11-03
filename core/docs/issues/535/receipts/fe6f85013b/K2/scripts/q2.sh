#!/bin/bash
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k2
$S/cells.sh $S/q2.log microsandbox lfs535k2m sqlite - C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q2.log microsandbox lfs535k2m pg-loop m C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q2.log microsandbox lfs535k2m pg-remote mr C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q2.log ssh lfs535k2s sqlite - C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
echo "Q2 ALL DONE" >> $S/q2.log
