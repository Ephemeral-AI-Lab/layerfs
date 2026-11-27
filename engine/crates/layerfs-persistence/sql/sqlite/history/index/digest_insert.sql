INSERT INTO file_digest(digest, length, root) VALUES ($1, $2, $3) ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root;
