CREATE TABLE catalog_state (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 publication INTEGER NOT NULL CHECK(publication>=0),
 next_ordinal INTEGER NOT NULL CHECK(next_ordinal BETWEEN 1 AND 4294967296),
 metadata_window_start INTEGER NOT NULL DEFAULT 1 CHECK(metadata_window_start>0),
 metadata_window_values INTEGER NOT NULL DEFAULT 0 CHECK(metadata_window_values BETWEEN 0 AND 131072)
);
INSERT INTO catalog_state(singleton,publication,next_ordinal) VALUES(1,0,1);
CREATE TABLE saves (
 save_id INTEGER PRIMARY KEY AUTOINCREMENT,
 status INTEGER NOT NULL DEFAULT 0 CHECK(status BETWEEN 0 AND 4),
 pending INTEGER NOT NULL DEFAULT 0 CHECK(pending>=0),
 publication INTEGER UNIQUE CHECK(publication>0)
);
CREATE TABLE bodies (
 body_order INTEGER PRIMARY KEY AUTOINCREMENT,
 domain INTEGER NOT NULL CHECK(domain IN (0,1)),
 save_id INTEGER NOT NULL REFERENCES saves,
 digest BLOB NOT NULL CHECK(length(digest)=32),
 metadata BLOB,
 ready INTEGER NOT NULL DEFAULT 0 CHECK(ready IN(0,1)),
 CHECK((domain=0 AND metadata IS NULL) OR
       (domain=1 AND ((ready=0 AND metadata IS NULL) OR (metadata IS NOT NULL AND length(metadata) BETWEEN 32 AND 1048576)))),
 UNIQUE(body_order,domain)
);
CREATE TABLE body_transfers (
 body_order INTEGER PRIMARY KEY REFERENCES bodies(body_order) ON DELETE CASCADE,
 received INTEGER NOT NULL DEFAULT 0 CHECK(received>=0),
 total INTEGER NOT NULL CHECK(total BETWEEN 32 AND 1048576),
 CHECK(received<=total)
);
CREATE UNIQUE INDEX body_ready_digest ON bodies(save_id,domain,digest) WHERE ready=1;
CREATE INDEX bodies_save ON bodies(save_id,body_order);
CREATE TABLE identities (
 id BLOB PRIMARY KEY CHECK(length(id)=32),
 role INTEGER NOT NULL,
 canonical_length INTEGER NOT NULL CHECK(canonical_length>0)
) WITHOUT ROWID;
CREATE TABLE locators (
 locator_id INTEGER PRIMARY KEY,
 id BLOB NOT NULL REFERENCES identities,
 domain INTEGER NOT NULL CHECK(domain IN (0,1)),
 body_order INTEGER NOT NULL,
 group_number INTEGER NOT NULL CHECK(group_number BETWEEN 0 AND 255),
 record_number INTEGER NOT NULL CHECK(record_number BETWEEN 0 AND 8190),
 FOREIGN KEY(body_order,domain) REFERENCES bodies(body_order,domain),
 UNIQUE(id,domain,body_order), UNIQUE(body_order,group_number,record_number)
);
CREATE INDEX locators_domain_id ON locators(domain,id,body_order);
CREATE TABLE logical_uses (
 locator_id INTEGER NOT NULL REFERENCES locators,
 logical_use INTEGER NOT NULL CHECK(logical_use IN (0,1)),
 save_id INTEGER NOT NULL REFERENCES saves,
 PRIMARY KEY(locator_id,logical_use,save_id)
) WITHOUT ROWID;
CREATE TABLE object_references (
 locator_id INTEGER NOT NULL,
 logical_use INTEGER NOT NULL,
 save_id INTEGER NOT NULL REFERENCES saves,
 child_id BLOB NOT NULL REFERENCES identities,
 child_domain INTEGER NOT NULL CHECK(child_domain IN (0,1)),
 child_use INTEGER NOT NULL CHECK(child_use IN (0,1)),
 PRIMARY KEY(locator_id,logical_use,save_id,child_id,child_domain,child_use),
 FOREIGN KEY(locator_id,logical_use,save_id) REFERENCES logical_uses(locator_id,logical_use,save_id)
) WITHOUT ROWID;
CREATE INDEX reference_child ON object_references(child_domain,child_id);
CREATE TABLE dependencies (
 locator_id INTEGER PRIMARY KEY REFERENCES locators,
 base_id BLOB NOT NULL REFERENCES identities,
 domain INTEGER NOT NULL CHECK(domain IN (0,1)),
 base_order INTEGER NOT NULL,
 FOREIGN KEY(base_order,domain) REFERENCES bodies(body_order,domain)
);
CREATE INDEX dependency_base ON dependencies(domain,base_order);
CREATE TABLE ordinal_reservations (
 first_ordinal INTEGER PRIMARY KEY CHECK(first_ordinal>0),
 count INTEGER NOT NULL CHECK(count>0),
 save_id INTEGER NOT NULL REFERENCES saves,
 CHECK(first_ordinal+count<=4294967296)
);
CREATE TABLE metadata_value_groups (
 first_ordinal INTEGER PRIMARY KEY CHECK(first_ordinal>0),
 count INTEGER NOT NULL CHECK(count BETWEEN 1 AND 165),
 body_order INTEGER NOT NULL REFERENCES bodies,
 group_number INTEGER NOT NULL CHECK(group_number BETWEEN 0 AND 255),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 UNIQUE(body_order,group_number)
);
CREATE TABLE candidate_entries (
 domain INTEGER NOT NULL CHECK(domain IN(0,1)),
 slot INTEGER NOT NULL CHECK(slot BETWEEN 0 AND 8191),
 stamp INTEGER NOT NULL CHECK(stamp>0),
 object_id BLOB NOT NULL REFERENCES identities,
 signature BLOB NOT NULL CHECK(length(signature)=32),
 save_id INTEGER NOT NULL REFERENCES saves,
 PRIMARY KEY(domain,slot,save_id),
 CHECK(slot=(stamp-1)%8192)
) WITHOUT ROWID;
CREATE INDEX candidate_stamp ON candidate_entries(domain,stamp,slot);
CREATE INDEX candidate_slot ON candidate_entries(domain,slot,stamp DESC);

CREATE TABLE candidate_slots (
 domain INTEGER NOT NULL CHECK(domain IN(0,1)),
 slot INTEGER NOT NULL CHECK(slot BETWEEN 0 AND 8191),
 PRIMARY KEY(domain,slot)
) WITHOUT ROWID;
CREATE UNIQUE INDEX candidate_identity_stamp ON candidate_entries(domain,stamp);
CREATE INDEX ordinal_reservation_save ON ordinal_reservations(save_id,first_ordinal);
CREATE TABLE save_context (
 save_id INTEGER PRIMARY KEY REFERENCES saves,
 workspace BLOB NOT NULL CHECK(length(workspace) BETWEEN 1 AND 255),
 incarnation BLOB NOT NULL CHECK(length(incarnation)=32),
 project BLOB NOT NULL CHECK(length(project)=17),
 branch BLOB NOT NULL CHECK(length(branch)=17),
 generation INTEGER NOT NULL CHECK(generation>0),
 scope BLOB NOT NULL CHECK(length(scope)=32),
 profile BLOB NOT NULL CHECK(length(profile)=32),
 base_root BLOB NOT NULL CHECK(length(base_root)=32),
 owned_pending INTEGER NOT NULL DEFAULT 1 CHECK(owned_pending IN(0,1))
);
CREATE UNIQUE INDEX owned_pending_workspace ON save_context(workspace,incarnation) WHERE owned_pending=1;
CREATE TRIGGER abandon_owned_capture AFTER UPDATE OF status ON saves WHEN NEW.status=4 BEGIN
 UPDATE save_context SET owned_pending=0 WHERE save_id=NEW.save_id;
END;
CREATE INDEX logical_use_save ON logical_uses(save_id,locator_id,logical_use);
