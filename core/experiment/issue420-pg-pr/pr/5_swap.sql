-- Retire the plain tables (content_signature's FK to object_location goes with it: not declarable on ol_a2).
\set ON_ERROR_STOP 1
\timing on
SELECT (SELECT count(*) FROM object_location) ol, (SELECT count(*) FROM ol_a2) ol_a2,
  (SELECT count(*) FROM inode_value) iv, (SELECT sum(bit_count(present::bit(64))) FROM iv_b32z) iv_b32z,
  (SELECT sum(10 + octet_length(name)) FROM binding_hint) bh_bytes, (SELECT sum(octet_length(entries)) FROM bh_b3) bh_b3_bytes,
  (SELECT count(*) FROM pack) pk, (SELECT count(*) FROM pk_a4) pk_a4, (SELECT count(*) FROM file_digest) fd, (SELECT count(*) FROM fd_a1) fd_a1;
DROP TABLE object_location CASCADE;
DROP TABLE inode_value, binding_hint, file_digest;
DROP TABLE pack CASCADE;
CREATE TRIGGER pack_immutable BEFORE UPDATE OR DELETE ON pk_a4 FOR EACH ROW EXECUTE FUNCTION pack_immutable();
VACUUM (ANALYZE);
CHECKPOINT;
