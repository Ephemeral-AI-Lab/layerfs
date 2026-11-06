INSERT INTO binding_hint_block(grp, seq, body) SELECT $1::bigint, coalesce(max(seq), 0) + 1, $2::bytea FROM binding_hint_block WHERE grp = $1::bigint;
