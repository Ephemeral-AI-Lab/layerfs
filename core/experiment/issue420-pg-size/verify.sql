-- Content checks: every bucketed layout holds exactly the base table's rows (same Commits applied to both).
SELECT 'ol rows', (SELECT count(*) FROM object_location) base, (SELECT sum(octet_length(entries)) / 46 FROM ol_b16) b16;
SELECT 'ol 10k keys found byte-exact', count(*) FROM ol_keys k JOIN object_location o USING (object_id)
  JOIN ol_b16 b ON b.bucket = get_byte(k.object_id, 0) * 256 + get_byte(k.object_id, 1)
  WHERE position(ol_entry(o.object_id, o.role, o.canonical_length, o.pack_id, o.group_number, o.record_number) IN b.entries) > 0;
SELECT 'iv rows', (SELECT count(*) FROM inode_value) base,
  (SELECT sum(bit_count(present::bit(32))) FROM iv_b18) b18, (SELECT sum(bit_count(present::bit(64))) FROM iv_b32z) b32z,
  (SELECT sum(octet_length(vals)) / 73 FROM iv_b32z) b32z_bytes;
SELECT 'iv 10k values equal (b32z, b18)',
  count(*) FILTER (WHERE substr(z.vals, 73 * bit_count((z.present & ((1::int8 << (k.ordinal % 32)::int) - 1))::bit(64))::int + 1, 73) = v.value),
  count(*) FILTER (WHERE substr(e.vals, 73 * (k.ordinal % 18)::int + 1, 73) = v.value AND e.present & (1 << (k.ordinal % 18)::int) <> 0)
FROM iv_keys k JOIN inode_value v USING (ordinal) JOIN iv_b32z z ON z.bucket = k.ordinal / 32 JOIN iv_b18 e ON e.bucket = k.ordinal / 18;
SELECT 'bh rows', (SELECT count(*) FROM binding_hint) base, (SELECT sum(octet_length(entries)) FROM bh_b2) b2_bytes, (SELECT sum(octet_length(entries)) FROM bh_b3) b3_bytes,
  (SELECT sum(10 + octet_length(name)) FROM binding_hint) expected_bytes;
SELECT 'bh rows of 10k children found (b2, b3)',
  count(*) FILTER (WHERE position(b1((h.child % 16)::int) || int8send(h.parent) || b1(length(h.name)) || h.name IN b2.entries) > 0),
  count(*) FILTER (WHERE position(b1((h.child % 128)::int) || int8send(h.parent) || b1(length(h.name)) || h.name IN b3.entries) > 0), count(*)
FROM bh_keys k JOIN binding_hint h USING (child) JOIN bh_b2 b2 ON b2.bucket = h.child / 16 JOIN bh_b3 b3 ON b3.bucket = h.child / 128;
SELECT 'pack bodies equal (a1, slices) of 2k keys', count(*) FILTER (WHERE a.body = p.body),
  count(*) FILTER (WHERE (SELECT string_agg(data, ''::bytea ORDER BY seq) FROM pk_b_slice s WHERE s.pack_id = p.pack_id) = p.body)
FROM pk_keys k JOIN pack p USING (pack_id) JOIN pk_a1 a USING (pack_id);
