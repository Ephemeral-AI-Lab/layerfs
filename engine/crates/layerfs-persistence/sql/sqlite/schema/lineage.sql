CREATE TABLE layer_lineage (
    layer_id BLOB PRIMARY KEY
        CHECK (length(layer_id) = 33 AND substr(layer_id, 1, 1) = X'32'),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17),
    height INTEGER NOT NULL CHECK (height >= 0),
    overwrote_count INTEGER NOT NULL CHECK (overwrote_count >= 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE commit_lineage (
    commit_id BLOB PRIMARY KEY
        CHECK (length(commit_id) = 33 AND substr(commit_id, 1, 1) = X'12'),
    run BLOB NOT NULL CHECK (length(run) = 33),
    depth INTEGER NOT NULL CHECK (depth >= 1),
    newest INTEGER NOT NULL CHECK (newest >= 0),
    merged BLOB CHECK (merged IS NULL OR length(merged) = 33)
) STRICT, WITHOUT ROWID;

CREATE TABLE absorbed (
    run BLOB NOT NULL CHECK (length(run) = 33),
    depth INTEGER NOT NULL CHECK (depth >= 1),
    height INTEGER NOT NULL CHECK (height >= 0),
    PRIMARY KEY (run, depth)
) STRICT, WITHOUT ROWID;

CREATE TABLE reach (
    key BLOB NOT NULL,
    x INTEGER NOT NULL CHECK (x >= 0),
    height INTEGER NOT NULL CHECK (height >= 0),
    PRIMARY KEY (key, x)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_heights ON layer_lineage(layer_stack_id, height);
CREATE UNIQUE INDEX commit_runs ON commit_lineage(run, depth);
CREATE INDEX run_merges ON commit_lineage(run, depth) WHERE merged IS NOT NULL;
CREATE INDEX absorbed_heights ON absorbed(run, height);
