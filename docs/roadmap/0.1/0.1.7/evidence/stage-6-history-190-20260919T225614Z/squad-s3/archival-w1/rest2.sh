#!/bin/bash
cd /tmp/w1codec
R=/tmp/w1/b0/records.bin
echo "== estimates ==" > /tmp/w1/rest2.txt
./target/release/w1codec estimate x 5,9,19 65536,262142,131071 >> /tmp/w1/rest2.txt 2>&1
echo "== levels_static 4MiB ==" >> /tmp/w1/rest2.txt
./target/release/w1codec levels_static $R 9,19 18 4194304 >> /tmp/w1/rest2.txt 2>>/tmp/w1/rest2.err
echo "== groups ordinary ==" >> /tmp/w1/rest2.txt
./target/release/w1codec groups $R 1,3,5,9,12,15,19,22 x /tmp/w1/b0/groups_ordinary.bin >> /tmp/w1/rest2.txt 2>>/tmp/w1/rest2.err
echo "== groups pooled ==" >> /tmp/w1/rest2.txt
./target/release/w1codec groups $R 1,3,5,9,12,15,19,22 x /tmp/w1/b0/groups_pooled-metadata.bin >> /tmp/w1/rest2.txt 2>>/tmp/w1/rest2.err
echo DONE >> /tmp/w1/rest2.txt
