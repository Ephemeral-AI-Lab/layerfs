#!/bin/bash
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k2
$S/cells.sh $S/q1.log apple-container lfs535k2a sqlite - C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q1.log apple-container lfs535k2a pg-loop a C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q1.log apple-container lfs535k2a pg-remote ar C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q1.log ssh lfs535k2s pg-remote sr C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
echo "Q1 ALL DONE" >> $S/q1.log
