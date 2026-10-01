/* Profile5: AliasFacts6/AliasJobs7 coexist with immutable Sites. */
CREATE TABLE alias_owner (
    id INTEGER PRIMARY KEY CHECK(id=1),
    scope BLOB NOT NULL CHECK(length(scope)=89),
    members BLOB CHECK(members IS NULL OR length(members)=164),
    stage INTEGER NOT NULL CHECK(stage BETWEEN 0 AND 4),
    sequence INTEGER NOT NULL CHECK(sequence>=0),
    facts INTEGER NOT NULL CHECK(facts>=0),
    jobs INTEGER NOT NULL CHECK(jobs BETWEEN 0 AND facts),
    expanded INTEGER NOT NULL CHECK(expanded BETWEEN 0 AND facts),
    current_serial INTEGER CHECK(current_serial>0),
    current_sequence INTEGER CHECK(current_sequence>0),
    progress BLOB NOT NULL CHECK(length(progress)=264),
    remaining INTEGER NOT NULL CHECK(remaining BETWEEN 0 AND facts),
    after_serial INTEGER CHECK(after_serial>0),
    CHECK((current_serial IS NULL)=(current_sequence IS NULL))
) STRICT, WITHOUT ROWID;
CREATE TABLE alias_facts (
    key BLOB PRIMARY KEY CHECK(length(key)=25),
    sequence INTEGER NOT NULL CHECK(sequence>0),
    status INTEGER NOT NULL CHECK(status BETWEEN 1 AND 3)
) STRICT, WITHOUT ROWID;
CREATE TABLE alias_jobs (
    key BLOB PRIMARY KEY CHECK(length(key)=25),
    serial INTEGER NOT NULL CHECK(serial>0)
) STRICT, WITHOUT ROWID;
