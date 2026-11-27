INSERT INTO binding_hint_block(grp, seq, body)
SELECT n.g, coalesce((SELECT max(b.seq) FROM binding_hint_block b WHERE b.grp = n.g), 0)
 + row_number() OVER (PARTITION BY n.g ORDER BY n.o), n.body
FROM unnest($1::bigint[], $2::bytea[]) WITH ORDINALITY n(g, body, o);
