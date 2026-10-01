/* Private metadata-only LFCS profile6; no payload CAS or Store schema. */
PRAGMA page_size=4096;
PRAGMA application_id=1279673171;
PRAGMA user_version=6;
CREATE TABLE session_owner(
 id INTEGER PRIMARY KEY CHECK(id=1),header BLOB NOT NULL CHECK(length(header)=216),
 scope BLOB CHECK(scope IS NULL OR length(scope)=81),sealed INTEGER NOT NULL CHECK(sealed=0),
 records INTEGER NOT NULL CHECK(records=0),record_bytes INTEGER NOT NULL CHECK(record_bytes=0),
 digest BLOB CHECK(digest IS NULL)
) STRICT,WITHOUT ROWID;
CREATE TABLE draft_owner(
 id INTEGER PRIMARY KEY CHECK(id=1),scope BLOB NOT NULL CHECK(length(scope)=105),
 stage INTEGER NOT NULL CHECK(stage IN(0,1)),records INTEGER NOT NULL CHECK(records BETWEEN 0 AND 65536),
 bytes INTEGER NOT NULL CHECK(bytes BETWEEN 0 AND 8388607),next_job INTEGER NOT NULL CHECK(next_job>0),
 selected BLOB CHECK(selected IS NULL OR length(selected)=32)
) STRICT,WITHOUT ROWID;
CREATE TABLE draft_headers(
 id BLOB PRIMARY KEY CHECK(length(id)=32),form INTEGER NOT NULL CHECK(form IN(0,1)),
 role INTEGER NOT NULL CHECK(role IN(3,4)),stage INTEGER NOT NULL CHECK(stage IN(0,1,2)),
 refs INTEGER NOT NULL CHECK(refs BETWEEN 0 AND 128),preds INTEGER NOT NULL CHECK(preds BETWEEN 0 AND 4),
 cursor INTEGER NOT NULL CHECK(cursor BETWEEN 0 AND 128),charge INTEGER NOT NULL CHECK(charge>0),
 queued INTEGER CHECK(queued IS NULL OR queued>0),digest BLOB NOT NULL CHECK(length(digest)=32)
) STRICT,WITHOUT ROWID;
CREATE TABLE draft_bodies(id BLOB PRIMARY KEY CHECK(length(id)=32),value BLOB NOT NULL CHECK(length(value)<=8192)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_references(id BLOB NOT NULL CHECK(length(id)=32),ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 127),value BLOB NOT NULL CHECK(length(value)=32),linked INTEGER NOT NULL CHECK(linked IN(0,1)),PRIMARY KEY(id,ordinal)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_predecessors(id BLOB NOT NULL CHECK(length(id)=32),ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 3),value BLOB NOT NULL CHECK(length(value)=32),provenance INTEGER NOT NULL CHECK(provenance BETWEEN 0 AND 2),PRIMARY KEY(id,ordinal)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_counts(id BLOB PRIMARY KEY CHECK(length(id)=32),links BLOB NOT NULL CHECK(length(links)=8)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_jobs(sequence INTEGER PRIMARY KEY CHECK(sequence>0),id BLOB NOT NULL CHECK(length(id)=32)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_committed(id BLOB PRIMARY KEY CHECK(length(id)=32),canonical BLOB NOT NULL CHECK(length(canonical)=32)) STRICT,WITHOUT ROWID;
CREATE TABLE draft_emissions(canonical BLOB PRIMARY KEY CHECK(length(canonical)=32),accepted INTEGER NOT NULL CHECK(accepted IN(0,1)),pending_draft BLOB CHECK(pending_draft IS NULL OR length(pending_draft)=32),CHECK((accepted=0 AND pending_draft IS NOT NULL) OR (accepted=1 AND pending_draft IS NULL))) STRICT,WITHOUT ROWID;
