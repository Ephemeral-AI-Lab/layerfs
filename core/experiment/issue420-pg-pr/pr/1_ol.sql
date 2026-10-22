-- pr layout (storage study, issue420-pg-size/publish_layouts.sql + a_relaxed.sql), built from the plain port.
-- object_location -> ol_a2: fixed-width first, narrow ints, hash exclusion instead of the btree PK.
\set ON_ERROR_STOP 1
\timing on
CREATE FUNCTION b1(int) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql AS $$ SELECT decode(lpad(to_hex($1), 2, '0'), 'hex') $$;
CREATE TABLE ol_a2 (pack_id bigint NOT NULL, canonical_length int4 NOT NULL, role int2 NOT NULL,
  group_number int2 NOT NULL, record_number int2 NOT NULL, object_id bytea NOT NULL);
INSERT INTO ol_a2 SELECT pack_id, canonical_length, role, group_number, record_number, object_id FROM object_location ORDER BY object_id;
ALTER TABLE ol_a2 ADD CONSTRAINT ol_a2_unique EXCLUDE USING hash (object_id WITH =);
