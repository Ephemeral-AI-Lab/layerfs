SELECT nextval('seq_iv_b18') AS c \gset
INSERT INTO iv_b18 (bucket, present, vals) SELECT bucket, present, vals FROM st_iv_b18 WHERE c = :c
ON CONFLICT (bucket) DO UPDATE SET present = iv_b18.present | excluded.present, vals = iv18_merge(iv_b18.vals, excluded.vals, excluded.present);
