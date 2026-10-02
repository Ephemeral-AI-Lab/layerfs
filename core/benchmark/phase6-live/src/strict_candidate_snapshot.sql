CREATE TEMP TABLE candidate_snapshot (
 stamp INTEGER PRIMARY KEY CHECK(stamp>0),
 slot INTEGER NOT NULL UNIQUE CHECK(slot BETWEEN 0 AND 8191),
 object_id BLOB NOT NULL CHECK(length(object_id)=32),
 signature BLOB NOT NULL CHECK(length(signature)=32)
);
CREATE TEMP TABLE candidate_snapshot_state (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 active INTEGER NOT NULL DEFAULT 0 CHECK(active IN(0,1)),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1)),
 engine BLOB CHECK(engine IS NULL OR length(engine)=8),
 publication INTEGER, ceiling INTEGER, own_save INTEGER, domain INTEGER,
 owner_eligible INTEGER NOT NULL DEFAULT 0 CHECK(owner_eligible IN(0,1)),
 expected_after INTEGER NOT NULL DEFAULT 0 CHECK(expected_after>=0),
 builds INTEGER NOT NULL DEFAULT 0,
 slot_lookups INTEGER NOT NULL DEFAULT 0,
 pages INTEGER NOT NULL DEFAULT 0,
 vm_steps INTEGER NOT NULL DEFAULT 0,
 page_vm_steps INTEGER NOT NULL DEFAULT 0,
 page_fullscan_steps INTEGER NOT NULL DEFAULT 0,
 page_sorts INTEGER NOT NULL DEFAULT 0
);
INSERT INTO candidate_snapshot_state(singleton) VALUES(1);
