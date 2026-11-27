WITH RECURSIVE src AS MATERIALIZED (
SELECT n.*,EXISTS (SELECT 1 FROM {s}.object_location x WHERE ('x'||encode(substring(x.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=n.k AND x.object_id=n.i OFFSET 0) AS lost 
FROM unnest($1::bytea[],$2::bigint[],$3::bigint[],$4::bigint[],$5::bigint[],$6::bigint[],$19::bigint[],$21::bytea[],$22::bigint[]) n(i,r,l,p,g,c,k,b,bk)), 
lists(i,a,d) AS (
SELECT s.i,CASE WHEN s.b IS NULL THEN ''::bytea ELSE (SELECT {s}.ancestor_entry(w.z,w.g,w.c,w.m)||w.a FROM (
SELECT ((x.pack_id-s.p)<<1)#((x.pack_id-s.p)>>63) AS z,x.group_number::bigint AS g,x.record_number::bigint AS c,x.canonical_length::bigint-1 AS m,x.ancestors AS a 
FROM {s}.object_location x WHERE ('x'||encode(substring(x.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=s.bk AND x.object_id=s.b OFFSET 0) w) END,1 
FROM src s WHERE NOT s.lost AND (s.b IS NULL OR NOT EXISTS (SELECT 1 FROM src t WHERE t.i=s.b AND NOT t.lost)) 
UNION ALL 
SELECT s.i,{s}.ancestor_entry(e.z,e.g,e.c,e.m)||l.a,l.d+1 FROM lists l JOIN src t ON t.i=l.i JOIN src s ON s.b=t.i AND NOT s.lost, 
LATERAL (SELECT ((t.p-s.p)<<1)#((t.p-s.p)>>63) AS z,t.g,t.c,t.l-1 AS m OFFSET 0) e WHERE l.d<64), 
ins AS (
INSERT INTO {s}.object_location(object_id,role,canonical_length,pack_id,group_number,record_number,ancestors) 
SELECT s.i,s.r,s.l,s.p,s.g,s.c,CASE WHEN octet_length(l.a)<=850 THEN l.a END FROM src s LEFT JOIN lists l ON l.i=s.i WHERE NOT s.lost 
RETURNING object_id,canonical_length), 
added AS (SELECT coalesce(sum(canonical_length),0)::bigint AS n FROM ins), 
totals AS (UPDATE {s}.store_totals SET canonical=store_totals.canonical+added.n,stored=store_totals.stored+$7::bigint,metadata=store_totals.metadata+$8::bigint 
FROM added WHERE store_totals.id=1 AND (added.n<>0 OR $7::bigint<>0 OR $8::bigint<>0)), 
values_ AS (INSERT INTO {s}.inode_value_block(first_ordinal,last_ordinal,body) 
SELECT n.f,n.l,n.b FROM unnest($9::bigint[],$18::bigint[],$10::bytea[]) n(f,l,b) WHERE NOT EXISTS (
SELECT 1 FROM {s}.inode_value_block x WHERE x.first_ordinal=(SELECT max(y.first_ordinal) FROM {s}.inode_value_block y 
WHERE y.first_ordinal<=n.l) AND x.last_ordinal>=n.f) RETURNING first_ordinal), 
released AS (UPDATE {s}.store_policy SET next_ordinal=$11::bigint WHERE $13::boolean AND id=1 AND next_ordinal=$11::bigint+$12::bigint), 
signed AS (INSERT INTO {s}.content_signature(slot,stamp,object_id,signature) 
SELECT * FROM unnest($14::bigint[],$15::bigint[],$16::bytea[],$17::bytea[]) 
ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature 
WHERE excluded.stamp>=content_signature.stamp), 
out AS (
SELECT object_id FROM ins 
UNION ALL SELECT NULL::bytea WHERE (SELECT count(*) FROM values_)<>cardinality($9::bigint[]) 
UNION ALL SELECT NULL::bytea WHERE EXISTS (SELECT 1 FROM unnest($16::bytea[],$20::bigint[]) s(i,k) 
WHERE NOT EXISTS (SELECT 1 FROM unnest($1::bytea[]) b(i) WHERE b.i=s.i) 
AND NOT EXISTS (SELECT 1 FROM {s}.object_location x WHERE ('x'||encode(substring(x.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=s.k AND x.object_id=s.i OFFSET 0))), 
o AS (SELECT count(object_id) AS n, count(*)-count(object_id) AS r FROM out) 
SELECT CASE WHEN current_setting('layerfs.attempt',true) IS DISTINCT FROM $23::text THEN (1/(o.n-o.n))::text 
WHEN o.r>0 THEN (32768+o.r)::smallint::text 
WHEN NOT $24::boolean AND o.n<>cardinality($1::bytea[]) THEN (1/(o.n-o.n))::text ELSE o.n::text END,
CASE WHEN $24::boolean THEN ARRAY(SELECT object_id FROM out WHERE object_id IS NOT NULL) END FROM o
