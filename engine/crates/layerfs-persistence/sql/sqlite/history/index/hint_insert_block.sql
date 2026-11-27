INSERT INTO binding_hint_block(grp, seq, body) SELECT $1, coalesce(max(seq), 0) + 1, $2 FROM binding_hint_block WHERE grp = $1;
