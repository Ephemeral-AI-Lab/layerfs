//! F44R3-1 (#44 round 3): every PostgreSQL one-statement read names the
//! Store's schema, and every one of them is proved against a real server.
//!
//! `read_statement` runs its one statement in autocommit with no
//! `search_path`, so a body that says `FROM branch` fails at runtime with
//! `Integrity("persistence")`. The guard below discovers every
//! `read_statement` body in this crate from its source, pins the statement
//! keys PostgreSQL runs for it, and fails when a body is added (or a key is
//! changed) until `backend/postgresql/history/statements.rs` serves a `{s}`
//! body for that key — in `qualified`, or in `arrays` for a column-batched
//! read.
//!
//! The PostgreSQL test below proves the pinned keys answer: it creates a
//! Store, commits and publishes, then reads it back through every
//! one-statement catalog read: `branch_any`, `branch_named`, `path_removals`,
//! `published_layer`,
//! `node_history_serial`, `layer` (layer.get), `layer_history` (layer.list),
//! `branch_snapshot`/`branch_snapshot_named` (the saved heads),
//! `commit_history` (a historical read), the project lookup and `statistics`.
//! It reports NOT_RUN without `LAYERFS_TEST_POSTGRES_URL`.
#![cfg(any(target_os = "macos", target_os = "linux"))]

#[path = "../support/mod.rs"]
mod support;

use layerfs_history::{
    CommitHistoryRequest, ContextTarget, HistoryCatalog, HistoryName, LayerHistoryRequest,
    NamedSnapshot, NodeId, NodeRecord, ReadContext,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// One `read_statement` body in this crate: the call its closure makes and the
/// statement keys PostgreSQL runs for it.
struct Read {
    /// The called function, by its last path segment (`branch::branch_any` is
    /// `branch_any`). `f` is the shared facade that forwards its caller's
    /// closure; `Transaction::new` is the session-lock shape, whose closure
    /// sends its PostgreSQL statement directly.
    called: &'static str,
    /// The statement keys PostgreSQL runs, in source order.
    keys: &'static [&'static str],
    /// The keys are PostgreSQL-only statements that name no table: they are
    /// served by `statements::statement`, not by `statements::qualified`.
    pg_only: bool,
}

/// Every `read_statement` body in this crate and the keys it runs. Adding or
/// removing a body, or changing the key a body sends, fails
/// [`every_one_statement_read_names_the_schema`] until this table and the
/// PostgreSQL statements agree.
const READS: &[Read] = &[
    Read {
        called: "layer_stack_named",
        keys: &["layerstack_stack_by_name"],
        pg_only: false,
    },
    Read {
        called: "branch_any",
        keys: &["branch_branch_by_id_any"],
        pg_only: false,
    },
    Read {
        called: "branch_named",
        keys: &["branch_branch_by_name"],
        pg_only: false,
    },
    Read {
        called: "branch_snapshot",
        keys: &["branch_snapshot"],
        pg_only: false,
    },
    Read {
        called: "branch_snapshot_with_stack_name",
        keys: &["branch_snapshot"],
        pg_only: false,
    },
    Read {
        called: "branch_snapshot_named",
        keys: &["branch_snapshot_named"],
        pg_only: false,
    },
    Read {
        called: "commit",
        keys: &["commit_commit_by_id"],
        pg_only: false,
    },
    Read {
        called: "layer",
        keys: &["layerstack_layer_by_id"],
        pg_only: false,
    },
    Read {
        called: "published_source",
        keys: &["layerstack_layer_by_source"],
        pg_only: false,
    },
    Read {
        called: "node_serial",
        keys: &["position_node_serial"],
        pg_only: false,
    },
    Read {
        called: "markers",
        keys: &["paths_markers"],
        pg_only: false,
    },
    Read {
        called: "node_markers",
        keys: &["paths_node_markers"],
        pg_only: false,
    },
    Read {
        called: "stored",
        keys: &["digest_roots"],
        pg_only: false,
    },
    Read {
        called: "read_context",
        keys: &["read_context_named", "read_context_node"],
        pg_only: false,
    },
    // First-parent windows (#818 H4); `first_parent_window` runs it on a
    // real server.
    Read {
        called: "chain_window",
        keys: &["lineage_chain_window"],
        pg_only: false,
    },
    // Init's acquisition reads (#818 I3a); `postgresql_acquisition_reads`
    // runs each one on a real server against SQLite's answers.
    Read {
        called: "unplaced_children",
        keys: &["acquisition_unplaced"],
        pg_only: false,
    },
    Read {
        called: "directories",
        keys: &["acquisition_directories"],
        pg_only: false,
    },
    Read {
        called: "directory_path",
        keys: &["acquisition_directory_path"],
        pg_only: false,
    },
    Read {
        called: "jobs",
        keys: &["acquisition_jobs"],
        pg_only: false,
    },
    Read {
        called: "job",
        keys: &["acquisition_job"],
        pg_only: false,
    },
    Read {
        called: "file_roots",
        keys: &["acquisition_file_roots"],
        pg_only: false,
    },
    Read {
        called: "entries",
        keys: &["acquisition_entries"],
        pg_only: false,
    },
    Read {
        called: "bindings",
        keys: &["acquisition_bindings"],
        pg_only: false,
    },
    Read {
        called: "work",
        keys: &["acquisition_owner"],
        pg_only: false,
    },
    // The shared facade forwards its caller's closure; the call sites above
    // are the bodies it runs.
    Read {
        called: "f",
        keys: &[],
        pg_only: false,
    },
    // The GC session keys and the owner's release are no `read_statement`
    // bodies since #768: `Mode::Bare` sends them as written
    // (`statements::bare`), and `postgresql_gc_sessions` runs each one on a
    // real server.
];

/// Every `.rs` file under `root`, as `(path, source)`.
fn rust_sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut sources = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                let source = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                sources.push((path, source));
            }
        }
    }
    sources
}

fn is_ident(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// True when `text` names `ident` as a whole identifier, so
/// `UNLOCK_SESSION_SHARED` does not name `LOCK_SESSION_SHARED`.
fn mentions(text: &str, ident: &str) -> bool {
    let mut from = 0;
    while let Some(at) = text[from..].find(ident) {
        let at = from + at;
        let end = at + ident.len();
        let before = at == 0 || !is_ident(text[..at].chars().next_back().unwrap_or(' '));
        let after = end == text.len() || !is_ident(text[end..].chars().next().unwrap_or(' '));
        if before && after {
            return true;
        }
        from = at + 1;
    }
    false
}

/// The byte of the `}` that closes the `{` at `open`, skipping string
/// literals.
fn braces(source: &str, open: usize) -> usize {
    nested(source, open, '{', '}')
}

/// The byte of the `)` that closes the `(` at `open`, skipping string
/// literals.
fn closing(source: &str, open: usize) -> usize {
    nested(source, open, '(', ')')
}

/// The byte of the `closer` that matches the `opener` at `open`, skipping
/// string literals.
fn nested(source: &str, open: usize, opener: char, closer: char) -> usize {
    let mut depth = 0usize;
    let mut quoted = false;
    for (offset, character) in source[open..].char_indices() {
        if quoted {
            if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            character if character == opener => depth += 1,
            character if character == closer => {
                depth -= 1;
                if depth == 0 {
                    return open + offset;
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced `{opener}` at byte {open}");
}

/// The `const NAME: &str = "key";` pairs of one source file.
fn constants(source: &str) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut rest = source;
    while let Some(at) = rest.find("const ") {
        let tail = &rest[at + "const ".len()..];
        let ident: String = tail
            .chars()
            .take_while(|character| is_ident(*character))
            .collect();
        if let Some(equals) = tail.find("= \"") {
            let value = &tail[equals + 3..];
            if let Some(end) = value.find('"') {
                found.insert(ident, value[..end].to_owned());
            }
        }
        rest = tail;
    }
    found
}

/// The call one `read_statement` closure argument makes, as written.
fn called(argument: &str) -> String {
    let after = match argument[1..].find('|') {
        Some(at) => &argument[at + 2..],
        None => argument,
    };
    let text =
        after.trim_start_matches(|character: char| character.is_whitespace() || character == '{');
    let chain: String = text
        .chars()
        .take_while(|character| is_ident(*character) || *character == ':')
        .collect();
    let chain = chain.trim_end_matches(':');
    assert!(
        !chain.is_empty(),
        "no call in the closure argument {argument:?}"
    );
    chain.to_owned()
}

/// The pinned name of a call: its last path segment, except the session-lock
/// shape, which is pinned whole so it cannot be mistaken for a function.
fn pinned_name(chain: &str) -> String {
    match chain {
        "Transaction::new" => chain.to_owned(),
        _ => chain.rsplit("::").next().expect("a call name").to_owned(),
    }
}

/// The `read_statement` and `session_statement` call sites of `sources`, as call name → the keys the
/// closure sends itself (only the session-lock shape names its statement).
fn call_sites(sources: &[(PathBuf, String)]) -> BTreeMap<String, BTreeSet<String>> {
    let mut sites: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // A session lock runs through `session_statement`, the unfenced form of
    // `read_statement` (#776 P2); both are one-statement reads.
    for (path, source) in sources {
        for caller in ["read_statement(", "session_statement("] {
            let mut from = 0;
            while let Some(at) = source[from..].find(caller) {
                let at = from + at;
                from = at + 1;
                // The definition, not a call.
                if source[..at].trim_end().ends_with("fn") {
                    continue;
                }
                let open = at + caller.len() - 1;
                let close = closing(source, open);
                let argument = &source[open + 1..close];
                let name = pinned_name(&called(argument));
                let mut keys = BTreeSet::new();
                if name == "Transaction::new" {
                    for (ident, key) in constants(source) {
                        if mentions(argument, &ident) {
                            keys.insert(key);
                        }
                    }
                    assert!(
                        !keys.is_empty(),
                        "{}: a session-lock closure names no statement key",
                        path.display()
                    );
                }
                sites.entry(name).or_default().extend(keys);
            }
        }
    }
    sites
}

/// The body of `fn <name>`, its braces included.
fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let signature = format!("fn {name}(");
    let at = source
        .find(&signature)
        .unwrap_or_else(|| panic!("no `{signature}` in statements.rs"));
    let open = at + source[at..].find('{').expect("a function body");
    let close = braces(source, open);
    &source[open..=close]
}

/// The `key => Some(include_str!("..."))` entries of one `statements.rs`
/// function, as key → the SQL text it names.
fn entries(source: &str, function: &str, directory: &Path) -> BTreeMap<String, String> {
    let body = function_body(source, function);
    let mut found = BTreeMap::new();
    let mut from = 0;
    while let Some(at) = body[from..].find("include_str!(") {
        let at = from + at;
        from = at + 1;
        let before = &body[..at];
        let close = before.rfind('"').expect("a statement key");
        let open = before[..close].rfind('"').expect("a statement key");
        let key = before[open + 1..close].to_owned();
        let argument_open = at + "include_str!".len();
        let argument_close = closing(body, argument_open);
        let argument = body[argument_open + 1..argument_close].trim();
        let relative = argument.trim_start_matches('"').trim_end_matches('"');
        let path = directory.join(relative);
        let sql = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        found.insert(key, sql);
    }
    found
}

/// The pinned, non-PostgreSQL-only keys, in source order.
fn pinned_keys() -> BTreeSet<&'static str> {
    READS
        .iter()
        .filter(|read| !read.pg_only)
        .flat_map(|read| read.keys.iter().copied())
        .collect()
}

/// The guard: every `read_statement` body's key has a `{s}` PostgreSQL body.
#[test]
fn every_one_statement_read_names_the_schema() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sites = call_sites(&rust_sources(&manifest.join("src")));
    let pinned: BTreeSet<&str> = READS.iter().map(|read| read.called).collect();
    let discovered: BTreeSet<&str> = sites.keys().map(String::as_str).collect();
    assert_eq!(
        discovered, pinned,
        "a `read_statement` body was added or removed: pin it in READS and give its key a `{{s}}` \
         body in src/backend/postgresql/history/statements.rs"
    );

    let statements_path = manifest.join("src/backend/postgresql/history/statements.rs");
    let statements = fs::read_to_string(&statements_path).expect("statements.rs");
    let directory = statements_path.parent().expect("the statements directory");
    let qualified = entries(&statements, "qualified", directory);
    let arrays = entries(&statements, "arrays", directory);

    // Every pinned key PostgreSQL runs is alone in one `{s}` statement, or is
    // a column-batched read whose array form names the schema.
    for read in READS.iter().filter(|read| !read.pg_only) {
        for key in read.keys {
            let sql = qualified
                .get(*key)
                .or_else(|| arrays.get(*key))
                .unwrap_or_else(|| {
                    panic!(
                    "`{}` runs `{key}` in one statement, but statements.rs has no `{{s}}` body for \
                     it: add sql/postgresql/queries/history/{key}.sql and a `qualified` entry",
                    read.called
                )
                });
            assert!(
                sql.contains("{s}."),
                "the PostgreSQL body of `{key}` names no schema: it must read `{{s}}.table`"
            );
        }
    }

    // No `qualified` entry is dead, and every one names the schema.
    let keys = pinned_keys();
    for (key, sql) in &qualified {
        assert!(
            keys.contains(key.as_str()),
            "`qualified` serves `{key}`, which no `read_statement` body runs: pin it in READS or \
             drop it"
        );
        assert!(
            sql.contains("{s}."),
            "the PostgreSQL body of `{key}` names no schema: it must read `{{s}}.table`"
        );
    }

    // The session-lock bodies are the documented exception: their statement is
    // PostgreSQL-only and reads no table, so there is nothing to qualify.
    let statement = function_body(&statements, "statement");
    for read in READS.iter().filter(|read| read.pg_only) {
        let sent: BTreeSet<String> = read.keys.iter().map(|key| (*key).to_owned()).collect();
        assert_eq!(
            sites[read.called], sent,
            "the `{}` bodies do not send the pinned keys",
            read.called
        );
        for key in read.keys {
            assert!(
                statement.contains(&format!("\"{key}\" =>")),
                "`{key}` is pinned as PostgreSQL-only, but `statement` does not serve it"
            );
            assert!(
                !qualified.contains_key(*key) && !arrays.contains_key(*key),
                "`{key}` has a `{{s}}` body, so it is not PostgreSQL-only"
            );
            let sqlite = manifest.join(format!("sql/sqlite/queries/history/{key}.sql"));
            assert!(
                !sqlite.exists(),
                "`{key}` is pinned as PostgreSQL-only, but has an unqualified SQLite body at {}",
                sqlite.display()
            );
        }
    }
}

const BINDING: &[u8] = b"postgresql-statement-schema";
const CURSOR: [u8; 32] = [44; 32];

/// The runtime evidence: every one-statement catalog read answers on a real
/// PostgreSQL Store, alone in its own snapshot.
#[test]
fn postgresql_one_statement_reads_answer() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = support::pg::Schema::new(url, "one_statement");
    let handles = schema.create(BINDING, CURSOR);
    let history = &handles.history;
    let (stack, branch, genesis) = support::pg::project(history, 1);
    let record = support::pg::commit(history, branch, 9);
    let layer = support::pg::publish(history, stack, branch);

    // branch_any: the by-id read without the visibility filter.
    let any = history
        .branch_any(branch)
        .expect("branch_any")
        .expect("the Branch row");
    assert_eq!(any.id, branch);
    assert_eq!(any.head_commit, Some(record.id));

    // published_layer: the Layer `branch` published for `commit`.
    assert_eq!(
        history
            .published_layer(branch, record.id)
            .expect("published_layer"),
        Some(layer.id)
    );

    // node_history_serial: one Layer and one Commit.
    assert!(
        history
            .node_history_serial(NodeId::Layer(genesis))
            .expect("the genesis Layer's serial")
            .is_some(),
        "a genesis Layer records its serial"
    );
    assert!(
        history
            .node_history_serial(NodeId::Commit(record.id))
            .expect("the Commit's serial")
            .is_some(),
        "a Commit records its serial"
    );

    // node_path_removals: a node's serial with its stack's markers, one
    // statement; a missing node is none.
    let serial = history
        .node_history_serial(NodeId::Commit(record.id))
        .expect("the Commit's serial")
        .expect("a Commit serial");
    assert_eq!(
        history
            .node_path_removals(stack, NodeId::Commit(record.id))
            .expect("no markers yet"),
        Some((serial, Vec::new()))
    );
    history
        .mark_path_removed(stack, b"b/gone", serial, 7)
        .expect("a marker");
    history
        .mark_path_removed(stack, b"a", serial, 8)
        .expect("a second marker");
    let (found, markers) = history
        .node_path_removals(stack, NodeId::Commit(record.id))
        .expect("the markers")
        .expect("the Commit");
    assert_eq!(found, serial);
    let paths: Vec<&[u8]> = markers.iter().map(|m| m.path.as_slice()).collect();
    assert_eq!(paths, [b"a".as_slice(), b"b/gone".as_slice()]);
    assert_eq!(
        (markers[1].frontier_serial, markers[1].removed_ms),
        (serial, 7)
    );
    assert_eq!(
        history
            .node_path_removals(stack, NodeId::Commit(absent_commit()))
            .expect("a missing node"),
        None
    );

    // path_removals: the stack's markers alone, one statement (#818 R1).
    let alone = history.path_removals(stack).expect("the markers alone");
    assert_eq!(alone, markers);

    // branch_named: the by-name read with the visibility filter (#818 R1); a
    // name the stack does not hold is none.
    let named = history
        .branch_named(stack, &support::name("work"))
        .expect("branch_named")
        .expect("the Branch row");
    assert_eq!((named.id, named.head_commit), (branch, Some(record.id)));
    assert_eq!(
        history
            .branch_named(stack, &support::name("absent"))
            .expect("a missing name"),
        None
    );

    // layer.get and layer.list.
    assert_eq!(
        history
            .layer(layer.id)
            .expect("layer.get")
            .expect("the Layer row")
            .source_branch,
        Some(branch)
    );
    let list = history
        .layer_history(&LayerHistoryRequest {
            stack,
            start: None,
            cursor: None,
            limit: 8,
        })
        .expect("layer.list");
    assert!(list.records.iter().any(|found| found.id == layer.id));

    // The saved heads: branch.get and the by-name lookup.
    let snapshot = history
        .branch_snapshot(branch)
        .expect("branch.get")
        .expect("the Branch snapshot");
    assert_eq!(snapshot.branch.id, branch);
    let (with_name, stack_name) = history
        .branch_snapshot_with_stack_name(branch)
        .expect("branch.get with its project")
        .expect("the Branch snapshot");
    assert_eq!(
        (with_name, stack_name.as_str()),
        (snapshot.clone(), "project-1")
    );
    assert!(matches!(
        history
            .branch_snapshot_named(&support::name("project-1"), &support::name("work"))
            .expect("a saved head by name"),
        NamedSnapshot::Found(_)
    ));

    // The project lookup, a historical Commit page and the Store totals.
    assert_eq!(
        history
            .layer_stack_named(&support::name("project-1"))
            .expect("the project by name")
            .expect("the project row")
            .id,
        stack
    );
    let commits = history
        .commit_history(&CommitHistoryRequest {
            branch,
            start: None,
            cursor: None,
            limit: 8,
        })
        .expect("a historical read");
    assert!(commits.records.iter().any(|found| found.id == record.id));
    assert!(
        history.statistics().expect("statistics").file > 0,
        "the Store's page total"
    );

    // read_context: one statement, decoded to what the default's reads and
    // the markers and recorded-Branch reads beside them find (#818 R5).
    let project = support::name("project-1");
    let targets = [
        ContextTarget::Main,
        ContextTarget::Branch(support::name("work")),
        ContextTarget::Branch(support::name("absent")),
        ContextTarget::Node(NodeId::Commit(record.id)),
        ContextTarget::Node(NodeId::Layer(layer.id)),
        ContextTarget::Node(NodeId::Commit(absent_commit())),
    ];
    for target in &targets {
        let found = history
            .read_context(&project, target)
            .expect("read_context");
        assert_eq!(
            found,
            expected(history, stack, &project, target),
            "{target:?}"
        );
    }
    let work = history
        .read_context(&project, &targets[1])
        .expect("read_context");
    let removals = work.removals.expect("read").expect("decoded");
    assert_eq!(removals.expect("the node").1, markers);
    assert_eq!(
        work.recorded.expect("read").expect("decoded").map(|b| b.id),
        Some(branch)
    );
    assert_eq!(
        history
            .read_context(&support::name("absent"), &ContextTarget::Main)
            .expect("no such project"),
        ReadContext::default()
    );
}

/// What `read_context` must answer: the default's reads, then the node's
/// markers and a Commit's recorded Branch, read one by one.
fn expected(
    history: &dyn HistoryCatalog,
    stack: layerfs_history::LayerStackId,
    project: &HistoryName,
    target: &ContextTarget,
) -> ReadContext {
    let mut context = layerfs_history::catalog::read_context_serial(history, project, target)
        .expect("the reads one by one");
    if let Some(node) = &context.node {
        let id = match node {
            NodeRecord::Commit(record) => NodeId::Commit(record.id),
            NodeRecord::Layer(record) => NodeId::Layer(record.id),
        };
        context.removals = Some(Ok(history.node_path_removals(stack, id).expect("markers")));
        context.recorded = Some(Ok(match node {
            NodeRecord::Commit(record) => record
                .meta
                .branch
                .and_then(|id| history.branch_any(id).expect("branch_any")),
            NodeRecord::Layer(_) => None,
        }));
    }
    context
}

/// A Commit id no Store holds.
fn absent_commit() -> layerfs_history::CommitId {
    let mut bytes = [0x5a; 33];
    bytes[0] = 0x12;
    layerfs_history::CommitId::from_bytes(bytes).expect("a Commit id")
}
