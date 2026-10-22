\set i random(1, 10000)
SELECT ordinal, value FROM inode_value WHERE ordinal BETWEEN (SELECT ordinal FROM iv_keys WHERE i = :i) AND (SELECT ordinal + 127 FROM iv_keys WHERE i = :i);
