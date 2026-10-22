\set i random(1, 10000)
SELECT h.child, h.parent, n.name FROM bh_b1 h JOIN bh_name n USING (name_id) WHERE h.child = (SELECT child FROM bh_keys WHERE i = :i);
