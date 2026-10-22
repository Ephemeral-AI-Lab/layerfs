\set i random(1, 10000)
SELECT value FROM inode_value WHERE ordinal = (SELECT ordinal FROM iv_keys WHERE i = :i);
