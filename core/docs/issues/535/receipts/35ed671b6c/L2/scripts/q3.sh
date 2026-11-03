#!/bin/bash
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l2
$S/cells.sh $S/q3.log ssh lfs535l2s sqlite - C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q3.log ssh lfs535l2s pg-remote sr C01 C02 C03 C04 C05 C06 C07 C08 C09 C10 C11 C12
$S/cells.sh $S/q3.log ssh lfs535l2s pg-loop s C01
echo "Q3 ALL DONE" >> $S/q3.log
