/* Private LFCS v4: sequential Sites, selected-budget effective graph, Roots. */
PRAGMA page_size = 4096;
PRAGMA application_id = 1279673171;
PRAGMA user_version = 4;

CREATE TABLE session_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    header BLOB NOT NULL CHECK (length(header) = 298),
    scope BLOB CHECK (scope IS NULL OR length(scope) = 81),
    sealed INTEGER NOT NULL CHECK (sealed IN (0, 1)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    record_bytes INTEGER NOT NULL CHECK (record_bytes = records * 63),
    digest BLOB CHECK (digest IS NULL OR length(digest) = 32),
    declared_roots INTEGER NOT NULL CHECK (declared_roots BETWEEN 0 AND 65536),
    declared_sites INTEGER NOT NULL CHECK (declared_sites BETWEEN 0 AND 65536),
    selected_bytes INTEGER NOT NULL CHECK (selected_bytes BETWEEN 16777216 AND 1099511623680 AND selected_bytes % 4096 = 0),
    graph_records INTEGER NOT NULL CHECK (graph_records = selected_bytes / 256 AND graph_records <= 4294967295),
    graph_record_bytes INTEGER NOT NULL CHECK (graph_record_bytes = graph_records * 63),
    CHECK (records <= declared_roots),
    CHECK ((sealed = 0 AND digest IS NULL) OR (sealed = 1 AND digest IS NOT NULL))
) STRICT, WITHOUT ROWID;

CREATE TABLE site_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    scope BLOB NOT NULL CHECK (length(scope) = 89),
    stage INTEGER NOT NULL CHECK (stage IN (0, 1, 2, 3, 4)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    remaining INTEGER NOT NULL CHECK (remaining BETWEEN 0 AND records),
    birth_digest BLOB CHECK (birth_digest IS NULL OR length(birth_digest) = 32),
    birth_max BLOB CHECK (birth_max IS NULL OR length(birth_max) = 25),
    final_digest BLOB CHECK (final_digest IS NULL OR length(final_digest) = 32),
    final_max BLOB CHECK (final_max IS NULL OR length(final_max) = 25),
    after_key BLOB CHECK (after_key IS NULL OR length(after_key) = 25),
    CHECK ((stage = 0 AND birth_digest IS NULL AND birth_max IS NULL AND remaining = 0)
        OR (stage > 0 AND birth_digest IS NOT NULL)),
    CHECK (stage < 2 OR final_digest IS NOT NULL),
    CHECK (stage != 4 OR remaining = 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE binding_sites (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    flags INTEGER NOT NULL CHECK (flags IN (0, 1, 3, 7)),
    point BLOB NOT NULL CHECK (length(point) = 28),
    parent INTEGER NOT NULL CHECK (parent BETWEEN 1 AND 9223372036854775807),
    binding_ordinal INTEGER NOT NULL CHECK (binding_ordinal BETWEEN 0 AND 4294967295)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX site_birth_order ON binding_sites(parent, binding_ordinal);
CREATE INDEX site_existing_parent ON binding_sites(parent, binding_ordinal)
    WHERE (flags&1)=1;

CREATE TABLE directory_roots (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal BETWEEN 1 AND 65536),
    root BLOB NOT NULL CHECK (length(root) = 32)
) STRICT, WITHOUT ROWID;

CREATE TABLE graph_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    scope BLOB NOT NULL CHECK (length(scope) = 188),
    stage INTEGER NOT NULL CHECK (stage BETWEEN 0 AND 8),
    nodes INTEGER NOT NULL CHECK (nodes BETWEEN 0 AND 4294967295),
    edges INTEGER NOT NULL CHECK (edges BETWEEN 0 AND 4294967295),
    record_bytes INTEGER NOT NULL CHECK (record_bytes = nodes * 60 + edges * 43),
    source_multiplicity BLOB NOT NULL CHECK (length(source_multiplicity) = 8),
    remaining_nodes INTEGER NOT NULL CHECK (remaining_nodes BETWEEN 0 AND nodes),
    remaining_edges INTEGER NOT NULL CHECK (remaining_edges BETWEEN 0 AND edges),
    adjacency_seal BLOB CHECK (adjacency_seal IS NULL OR length(adjacency_seal) = 285),
    proof_seal BLOB CHECK (proof_seal IS NULL OR length(proof_seal) = 317),
    max_node BLOB CHECK (max_node IS NULL OR length(max_node) = 25),
    max_edge BLOB CHECK (max_edge IS NULL OR length(max_edge) = 33),
    after_node BLOB CHECK (after_node IS NULL OR length(after_node) = 25),
    after_edge BLOB CHECK (after_edge IS NULL OR length(after_edge) = 33),
    seed_count INTEGER NOT NULL CHECK (seed_count BETWEEN 0 AND 65536),
    CHECK (nodes + edges <= 4294967295),
    CHECK (stage != 7 OR (remaining_nodes = 0 AND remaining_edges = 0))
) STRICT, WITHOUT ROWID;

CREATE TABLE solver_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    scope BLOB NOT NULL CHECK (length(scope) = 188),
    current_serial INTEGER NOT NULL CHECK (current_serial BETWEEN 0 AND 9223372036854775807),
    dfs_root_serial INTEGER NOT NULL CHECK (dfs_root_serial BETWEEN 0 AND 9223372036854775807),
    next_discovery INTEGER NOT NULL CHECK (next_discovery BETWEEN 1 AND 4294967296),
    scc_root_serial INTEGER NOT NULL CHECK (scc_root_serial BETWEEN 0 AND 9223372036854775807),
    scc_boundary INTEGER NOT NULL CHECK (scc_boundary BETWEEN 0 AND 4294967295),
    cumulative_pop_count INTEGER NOT NULL CHECK (cumulative_pop_count BETWEEN 0 AND 4294967295),
    last_stack_discovery INTEGER NOT NULL CHECK (last_stack_discovery BETWEEN 0 AND 4294967295),
    any_seed INTEGER NOT NULL CHECK (any_seed IN (0, 1)),
    singleton_self_loop INTEGER NOT NULL CHECK (singleton_self_loop IN (0, 1))
) STRICT, WITHOUT ROWID;

CREATE TABLE graph_nodes (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    value BLOB NOT NULL CHECK (length(value) = 29),
    flags INTEGER NOT NULL CHECK (flags BETWEEN 0 AND 127),
    discovery INTEGER NOT NULL CHECK (discovery BETWEEN 0 AND 4294967295)
) STRICT, WITHOUT ROWID;
CREATE INDEX graph_unexpanded ON graph_nodes(key) WHERE (flags&2)=0;
CREATE UNIQUE INDEX graph_discovery_stack ON graph_nodes(discovery) WHERE (flags&8)=8;

CREATE TABLE graph_edges (
    key BLOB PRIMARY KEY CHECK (length(key) = 33),
    multiplicity BLOB NOT NULL CHECK (length(multiplicity) = 4 AND multiplicity != x'00000000')
) STRICT, WITHOUT ROWID;
