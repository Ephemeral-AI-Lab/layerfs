-- Untimed, deterministic inputs for the speed diagnostics (same for every arm).
\set ON_ERROR_STOP 1
SELECT setseed(0.420);
CREATE TABLE ol_keys AS SELECT row_number() OVER ()::int AS i, object_id FROM (SELECT object_id FROM object_location ORDER BY random() LIMIT 10000) s;
CREATE TABLE iv_keys AS SELECT row_number() OVER ()::int AS i, ordinal FROM (SELECT ordinal FROM inode_value ORDER BY random() LIMIT 10000) s;
CREATE TABLE bh_keys AS SELECT row_number() OVER ()::int AS i, child FROM (SELECT child FROM (SELECT DISTINCT child FROM binding_hint) d ORDER BY random() LIMIT 10000) s;
CREATE TABLE pk_keys AS SELECT row_number() OVER ()::int AS i, pack_id FROM (SELECT pack_id FROM pack ORDER BY random() LIMIT 2000) s;
ALTER TABLE ol_keys ADD PRIMARY KEY (i); ALTER TABLE iv_keys ADD PRIMARY KEY (i);
ALTER TABLE bh_keys ADD PRIMARY KEY (i); ALTER TABLE pk_keys ADD PRIMARY KEY (i);
-- 323 real inode values and 322 real (parent, name) pairs to insert under fresh keys
CREATE TABLE iv_src AS SELECT row_number() OVER (ORDER BY ordinal)::int - 1 AS g, value FROM (SELECT ordinal, value FROM inode_value ORDER BY ordinal OFFSET 3000000 LIMIT 323) s;
CREATE TABLE bh_src AS SELECT row_number() OVER ()::int - 1 AS g, parent, name FROM (SELECT parent, name FROM binding_hint ORDER BY random() LIMIT 322) s;
-- three pack bodies of the median shape of a Commit (two small, one mid)
CREATE TABLE pk_src AS SELECT row_number() OVER ()::int - 1 AS g, domain, length, body FROM (
  (SELECT domain, length, body FROM pack WHERE length BETWEEN 300 AND 1500 ORDER BY pack_id DESC LIMIT 2)
  UNION ALL (SELECT domain, length, body FROM pack WHERE length BETWEEN 6000 AND 12000 ORDER BY pack_id DESC LIMIT 1)) s;
CREATE SEQUENCE commit_seq START 1;
SELECT count(*), sum(octet_length(value)) FROM iv_src; SELECT count(*) FROM bh_src; SELECT g, domain, length FROM pk_src;
