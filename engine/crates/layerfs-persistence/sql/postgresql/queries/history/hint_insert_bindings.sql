INSERT INTO binding_hint(child, parent, name) SELECT * FROM unnest($1::bigint[], $2::bigint[], $3::bytea[]) ON CONFLICT DO NOTHING;
