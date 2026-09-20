#!/bin/bash
cd /tmp/w1codec
R=/tmp/w1/armA/records.bin
echo "== levels (arm A population) ==" > /tmp/w1/armA_sweep.txt
./target/release/w1codec levels $R 1,3,5,9,12,15,19,22 18 >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo "== levels_min ==" >> /tmp/w1/armA_sweep.txt
./target/release/w1codec levels_min $R 3,9,19 18 >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo "== groups ordinary ==" >> /tmp/w1/armA_sweep.txt
./target/release/w1codec groups $R 1,3,5,9,12,15,19,22 x /tmp/w1/armA/groups_ordinary.bin >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo "== groups pooled ==" >> /tmp/w1/armA_sweep.txt
./target/release/w1codec groups $R 1,3,5,9,12,15,19,22 x /tmp/w1/armA/groups_pooled-metadata.bin >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo "== window @9 ==" >> /tmp/w1/armA_sweep.txt
./target/release/w1codec window $R 16,17,18,20 9 >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo "== roundtrip ==" >> /tmp/w1/armA_sweep.txt
./target/release/w1codec roundtrip $R 9 18 >> /tmp/w1/armA_sweep.txt 2>>/tmp/w1/armA_sweep.err
echo DONE >> /tmp/w1/armA_sweep.txt
