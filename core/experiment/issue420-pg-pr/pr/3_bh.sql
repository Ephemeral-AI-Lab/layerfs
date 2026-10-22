-- binding_hint -> bh_b3: 128 children per row, packed entries (child%128, parent, name length, name), lz4.
\set ON_ERROR_STOP 1
\timing on
CREATE TABLE bh_b3 (bucket bigint NOT NULL, entries bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE bh_b3 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO bh_b3 SELECT child / 128, string_agg(b1((child % 128)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name)
FROM binding_hint GROUP BY child / 128 ORDER BY 1;
ALTER TABLE bh_b3 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
