/* Profile7: immutable BaseFacts17 plus sealed ParentEligibility18 in one LFCS. */
CREATE TABLE fact_owner (
    table_id INTEGER PRIMARY KEY CHECK(table_id IN (17,18)),
    scope BLOB CHECK(scope IS NULL OR length(scope)=228),
    stage INTEGER NOT NULL CHECK(stage BETWEEN 0 AND 5),
    records INTEGER NOT NULL CHECK(records>=0),
    bound INTEGER NOT NULL CHECK(bound BETWEEN 0 AND records),
    record_bytes INTEGER NOT NULL CHECK(record_bytes=records*CASE table_id WHEN 17 THEN 105 ELSE 32 END),
    remaining INTEGER NOT NULL CHECK(remaining BETWEEN 0 AND records),
    after_serial INTEGER CHECK(after_serial>0),
    maximum INTEGER CHECK(maximum>0),
    digest BLOB CHECK(digest IS NULL OR length(digest)=32)
) STRICT, WITHOUT ROWID;
CREATE TABLE base_facts (
    key BLOB PRIMARY KEY CHECK(length(key)=25),
    value BLOB NOT NULL CHECK(length(value)=74)
) STRICT, WITHOUT ROWID;
CREATE TABLE parent_eligibility (
    key BLOB PRIMARY KEY CHECK(length(key)=25),
    bound INTEGER NOT NULL CHECK(bound IN (0,1))
) STRICT, WITHOUT ROWID;
INSERT INTO fact_owner VALUES(17,NULL,0,0,0,0,0,NULL,NULL,NULL);
INSERT INTO fact_owner VALUES(18,NULL,0,0,0,0,0,NULL,NULL,NULL);
