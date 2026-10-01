/* Private profile8 canonical counts and external descendant frontier only. */
CREATE TABLE count_owner(
 id INTEGER PRIMARY KEY CHECK(id=1),scope BLOB CHECK(scope IS NULL OR length(scope)=228),
 stage INTEGER NOT NULL CHECK(stage BETWEEN 0 AND 6),records INTEGER NOT NULL CHECK(records>=0),
 touched INTEGER NOT NULL CHECK(touched BETWEEN 0 AND records),maximum INTEGER CHECK(maximum>0),
 zeros INTEGER NOT NULL CHECK(zeros>=0),zero_maximum INTEGER CHECK(zero_maximum>0),
 count_remaining INTEGER NOT NULL CHECK(count_remaining BETWEEN 0 AND records),
 zero_remaining INTEGER NOT NULL CHECK(zero_remaining BETWEEN 0 AND zeros),
 after_count INTEGER CHECK(after_count>0),after_zero INTEGER CHECK(after_zero>0),
 effects BLOB CHECK(effects IS NULL OR length(effects)=294),
 final_seal BLOB CHECK(final_seal IS NULL OR length(final_seal)=294),
 seeds BLOB CHECK(seeds IS NULL OR length(seeds)=579)
) STRICT,WITHOUT ROWID;
CREATE TABLE canonical_counts(key BLOB PRIMARY KEY CHECK(length(key)=25),value BLOB NOT NULL CHECK(length(value)=97)) STRICT,WITHOUT ROWID;
CREATE TABLE zero_seeds(key BLOB PRIMARY KEY CHECK(length(key)=25),value BLOB NOT NULL CHECK(length(value)=74)) STRICT,WITHOUT ROWID;
CREATE TABLE release_owner(
 id INTEGER PRIMARY KEY CHECK(id=1),scope BLOB CHECK(scope IS NULL OR length(scope)=228),
 stage INTEGER NOT NULL CHECK(stage BETWEEN 0 AND 5),seeds BLOB CHECK(seeds IS NULL OR length(seeds)=579),
 seeded INTEGER NOT NULL CHECK(seeded>=0),seed_after INTEGER CHECK(seed_after>0),
 sequence INTEGER NOT NULL CHECK(sequence>=0),pending INTEGER NOT NULL CHECK(pending>=0),
 current_key BLOB CHECK(current_key IS NULL OR length(current_key)=25),
 current_value BLOB CHECK(current_value IS NULL OR length(current_value)=82),
 frames INTEGER NOT NULL CHECK(frames>=0),completed INTEGER NOT NULL CHECK(completed>=0),
 directories INTEGER NOT NULL CHECK(directories>=0),maximum_depth INTEGER NOT NULL CHECK(maximum_depth>=frames),
 CHECK((current_key IS NULL)=(current_value IS NULL))
) STRICT,WITHOUT ROWID;
CREATE TABLE release_jobs(key BLOB PRIMARY KEY CHECK(length(key)=25),value BLOB NOT NULL CHECK(length(value)=82)) STRICT,WITHOUT ROWID;
CREATE TABLE release_frames(key BLOB PRIMARY KEY CHECK(length(key)=25),value BLOB NOT NULL CHECK(length(value)=291)) STRICT,WITHOUT ROWID;
INSERT INTO count_owner VALUES(1,NULL,0,0,0,NULL,0,NULL,0,0,NULL,NULL,NULL,NULL,NULL);
INSERT INTO release_owner VALUES(1,NULL,0,NULL,0,NULL,0,0,NULL,NULL,0,0,0,0);
