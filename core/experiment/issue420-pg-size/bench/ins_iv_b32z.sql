SELECT nextval('seq_iv_b32z') AS c \gset
INSERT INTO iv_b32z (bucket, present, vals) SELECT bucket, present, vals FROM st_iv_b32 WHERE c = :c
ON CONFLICT (bucket) DO UPDATE SET present = iv_b32z.present | excluded.present, vals = iv_b32z.vals || excluded.vals;
