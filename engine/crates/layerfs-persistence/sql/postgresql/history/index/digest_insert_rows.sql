INSERT INTO file_digest(digest, length, root) SELECT * FROM unnest($1::bytea[], $2::bigint[], $3::bytea[]) ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root;
