\set ON_ERROR_STOP 1
\timing on
CREATE FUNCTION b1(int) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql AS $$ SELECT decode(lpad(to_hex($1), 2, '0'), 'hex') $$;
-- A1: same columns, lookup btree on child only, no uniqueness (INSERT OR IGNORE no longer exact)
CREATE TABLE bh_a1 (child bigint NOT NULL, parent bigint NOT NULL, name bytea NOT NULL);
INSERT INTO bh_a1 SELECT * FROM binding_hint ORDER BY child, parent, name;
CREATE INDEX bh_a1_child ON bh_a1 (child);
-- A2: same columns, uniqueness by hash exclusion on an injective key, plus btree(child) for lookups
CREATE FUNCTION bh_key(bigint, bigint, bytea) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql
  AS $$ SELECT int8send($1) || int8send($2) || $3 $$;
CREATE TABLE bh_a2 (child bigint NOT NULL, parent bigint NOT NULL, name bytea NOT NULL);
INSERT INTO bh_a2 SELECT * FROM binding_hint ORDER BY child, parent, name;
CREATE INDEX bh_a2_child ON bh_a2 (child);
ALTER TABLE bh_a2 ADD CONSTRAINT bh_a2_unique EXCLUDE USING hash (bh_key(child, parent, name) WITH =);
-- A3: int4 child/parent (current max 16.3 M; schema allows u64), unique btree
CREATE TABLE bh_a3 (child int4 NOT NULL, parent int4 NOT NULL, name bytea NOT NULL);
INSERT INTO bh_a3 SELECT * FROM binding_hint ORDER BY child, parent, name;
ALTER TABLE bh_a3 ADD PRIMARY KEY (child, parent, name);
-- B1: name dictionary, int8 serials kept
CREATE TABLE bh_name (name_id int4 NOT NULL, name bytea NOT NULL);
INSERT INTO bh_name SELECT row_number() OVER (ORDER BY name), name FROM (SELECT DISTINCT name FROM binding_hint) s;
ALTER TABLE bh_name ADD PRIMARY KEY (name_id);
ALTER TABLE bh_name ADD UNIQUE (name);
CREATE TABLE bh_b1 (child bigint NOT NULL, parent bigint NOT NULL, name_id int4 NOT NULL);
INSERT INTO bh_b1 SELECT h.child, h.parent, n.name_id FROM binding_hint h JOIN bh_name n USING (name) ORDER BY 1, 2, 3;
ALTER TABLE bh_b1 ADD PRIMARY KEY (child, parent, name_id);
-- B2: one row per 16 children: entries [child&15][parent u64][len][name] packed, uncompressed
CREATE TABLE bh_b2 (bucket bigint NOT NULL, entries bytea NOT NULL);
ALTER TABLE bh_b2 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO bh_b2 SELECT child / 16, string_agg(b1((child % 16)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name)
FROM binding_hint GROUP BY child / 16 ORDER BY 1;
ALTER TABLE bh_b2 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
-- B3: one row per 128 children, lz4 above the TOAST threshold
CREATE TABLE bh_b3 (bucket bigint NOT NULL, entries bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE bh_b3 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO bh_b3 SELECT child / 128, string_agg(b1((child % 128)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name)
FROM binding_hint GROUP BY child / 128 ORDER BY 1;
ALTER TABLE bh_b3 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
VACUUM (ANALYZE) bh_a1, bh_a2, bh_a3, bh_name, bh_b1, bh_b2, bh_b3;
