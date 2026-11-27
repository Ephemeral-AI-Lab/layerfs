//! §4.2 visibility guard: every shipped statement that reads `branch` or
//! `layer_stack` carries the retired-row `NOT EXISTS` filter unless it is on
//! the documented allow-list below.
//!
//! The statements are discovered from the shipped SQL directories rather than
//! from a hand-kept list, so a new statement that reads either table without
//! the filter fails here until it is filtered or allow-listed. The scan also
//! covers the crate's own Rust source: an inline `SELECT ... FROM`/`JOIN`
//! literal that names `branch` or `layer_stack` is held to the same filter, so
//! a read outside the `sql/` directories (such as `backend/sqlite/lifecycle/heads.rs`)
//! cannot bypass the guard. Every allow-list entry carries its reason and is
//! proved to still be a live statement that reads the table, so the list cannot
//! rot. The lookup pairs and the snapshot/live statements §4.2 names by hand are
//! also pinned, because a scan alone would not notice a deletion.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// One shipped statement file and the backend directory it came from.
struct Statement {
    backend: &'static str,
    file: String,
    sql: String,
}

/// The §4.2 statements that must carry the filter: the four Branch lookups,
/// both snapshots, the GC live set and the roots of the batched §6.1 closure.
const BRANCH_FILTERED: &[&str] = &[
    "gc/kept_node/seed.sql",
    "history/branch/by_id.sql",
    "history/branch/by_name.sql",
    "history/branch/first_page.sql",
    "history/branch/next_page.sql",
    "history/branch/snapshot.sql",
    "history/branch/snapshot_named.sql",
    "gc/mark/live_branches.sql",
];

/// The four project lookups, their live set and the PostgreSQL by-name
/// override (the reason the guard scans both backends).
const STACK_FILTERED: &[&str] = &[
    "gc/kept_node/seed.sql",
    "history/stack/by_id.sql",
    "history/stack/by_name.sql",
    "history/layerstack/stack_by_name.sql",
    "history/stack/first_page.sql",
    "history/stack/next_page.sql",
    "gc/mark/live_stacks.sql",
];

/// A statement that reads a filtered table without the filter, and why.
struct Allow {
    file: &'static str,
    /// Retired-row tables the statement must still name: an allow-listed
    /// statement that is supposed to see retired state must not stop seeing it.
    markers: &'static [&'static str],
    /// The name-collision checks return the holder's state with `CASE WHEN`.
    returns_state: bool,
    reason: &'static str,
}

/// §4.2's allow-list. Every entry is a decision, not a convenience: the reason
/// is the comment and the test proves the entry still reads the table.
const ALLOWED: &[Allow] = &[
    Allow {
        file: "gc/kept_node/expand.sql",
        markers: &[],
        returns_state: false,
        reason: "§6.1 edges of the batched history closure (#42 item 29). The retained \
                 roots are chosen by kept_node_seed.sql, which carries both markers and is \
                 pinned as a filtered statement above. This statement only follows the \
                 edges of nodes already in `kept_node`, and §6.1 requires a retired Branch \
                 that a retained Commit or Layer names (`commit.branch_id`, \
                 `layer.source_branch_id`) to stay with its head, base and fork point, so \
                 the Branch row it joins must be read whether retired or not. It is the \
                 edge half of the one recursive statement PostgreSQL still runs \
                 (kept_node_mark.sql), whose edges are equally unfiltered; \
                 tests/kept_node_closure.rs holds the two to the same rows",
    },
    Allow {
        file: "history/branch/by_id_any.sql",
        markers: &[],
        returns_state: false,
        reason: "§4.2 render/identity for layer.list, layer.get, saved and operator \
                 inspect: a collected Branch's own row is still rendered, so this by-id \
                 read is deliberately unfiltered; the filtered branch_branch_by_id is \
                 the lookup path",
    },
    Allow {
        file: "history/branch/name_taken.sql",
        markers: &["branch_retired"],
        returns_state: true,
        reason: "§4.2 name-collision check: it must see the retired holder and return \
                 its state so branch.fork answers NameTaken with the holder",
    },
    Allow {
        file: "gc/intent_scope/all_project.sql",
        markers: &[],
        returns_state: false,
        reason: "§5.1/§8 compaction admission: collects every open intent for a project, \
                 including intents scoped to its retired Branches, so a delete or \
                 compaction cannot leak one",
    },
    Allow {
        file: "gc/intent_scope/ops_project.sql",
        markers: &[],
        returns_state: false,
        reason: "§5.1/§8 admission: the operation subset of gc_intent_all_project \
                 (Init, merge, removal and compaction, but not another mount), with the \
                 same retired-Branch coverage",
    },
    Allow {
        file: "gc/unreachable_removal/reach.sql",
        markers: &[],
        returns_state: false,
        reason: "§4.2 compaction's rebuild: the unreachable-row cleanup reads which Stacks \
                 still exist so it drops only the `reach` rows of a collected Stack; it bounds \
                 one collection under the Store write lane and renders nothing",
    },
    Allow {
        file: "history/statistics/unreachable_count.sql",
        markers: &[],
        returns_state: false,
        reason: "§4.2 compaction's mark: the count of one Stack's unreachable rows reads \
                 `branch` and `layer_stack` by id to bound the collection the kept-node mark \
                 is about to run, including rows a retired Stack leaves behind; it renders \
                 nothing",
    },
    Allow {
        file: "history/stack/head.sql",
        markers: &[],
        returns_state: false,
        reason: "§4 add_layer: the StackMoved report reads the actual head after the \
                 filtered stack lookup already resolved a live Stack under the Store \
                 write lane; filtering would turn StackMoved into Integrity",
    },
    Allow {
        file: "history/stack/name_taken.sql",
        markers: &["layer_stack_retired"],
        returns_state: true,
        reason: "§4.2 project name-collision check: it must see the deleted holder and \
                 return its state so project.init answers NameTaken with the holder",
    },
    Allow {
        file: "history/node/read_context_node.sql",
        markers: &["layer_stack_retired"],
        returns_state: false,
        reason: "§4.2 render/identity, #818 R5: a read call's context by node in one \
                 statement. Its project lookup and marker rows keep the \
                 layer_stack_retired filter (a deleted project is no project); its one \
                 `branch` read is the node's recorded Branch name (`commit.branch_id`), \
                 which replaces branch_branch_by_id_any and is unfiltered for the same \
                 reason: a collected Branch's own row is still rendered in a ref. \
                 read_context_named.sql makes the same unfiltered recorded-Branch join \
                 beside its filtered by-name Branch lookup",
    },
];

/// The shipped history and GC statement directories of each backend (#828:
/// the former `queries/history/` set, grouped by table).
const STATEMENT_DIRECTORIES: &[(&str, &[&str])] = &[
    ("sqlite", &["history", "gc"]),
    ("postgresql", &["history", "gc", "acquisition"]),
];

/// The `.sql` files under `directory`, recursively.
fn sql_files(directory: &Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sql_files(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("sql") {
            out.push(path);
        }
    }
}

/// Every shipped statement, named by its path under `sql/<backend>/`.
fn read_statements() -> Vec<Statement> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("sql");
    let mut statements = Vec::new();
    for (backend, directories) in STATEMENT_DIRECTORIES {
        let base = root.join(backend);
        let mut paths = Vec::new();
        for directory in *directories {
            sql_files(&base.join(directory), &mut paths);
        }
        for path in paths {
            let file = path
                .strip_prefix(&base)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            let sql = fs::read_to_string(&path).unwrap();
            statements.push(Statement { backend, file, sql });
        }
    }
    statements.sort_by(|left, right| {
        (left.file.as_str(), left.backend).cmp(&(right.file.as_str(), right.backend))
    });
    statements
}

/// The SQL with its `--` comments removed, so a commented-out clause or a
/// trailing note cannot satisfy the filter check.
fn without_comments(sql: &str) -> String {
    sql.lines()
        .map(|line| line.split_once("--").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The statement's words, ignoring punctuation and quoting.
fn words(sql: &str) -> Vec<&str> {
    sql.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// The SQL with its schema placeholders removed: `{s}.branch` and
/// `{schema}.branch` become `branch`, and `{}.pack` becomes `pack`, so an
/// inline `format!` statement is scanned like a shipped one.
fn without_placeholders(sql: &str) -> String {
    let sql = without_comments(sql);
    let mut out = String::with_capacity(sql.len());
    let mut rest = sql.as_str();
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        let after = open + close + 1;
        if rest[after..].starts_with('.') {
            rest = &rest[after + 1..];
        } else {
            rest = &rest[after..];
        }
    }
    out.push_str(rest);
    out
}

/// Whether `sql` reads rows of `table` through a `FROM`/`JOIN` relation
/// (including a subquery), rather than writing to it. `DELETE FROM` and
/// an `UPDATE`/`INSERT` are not reads; `INSERT ... SELECT ... FROM` is.
fn reads(sql: &str, table: &str) -> bool {
    let normalized = without_placeholders(sql);
    let words = words(&normalized);
    words.iter().enumerate().any(|(index, word)| {
        let relation = word.eq_ignore_ascii_case("from") || word.eq_ignore_ascii_case("join");
        let deleted = index > 0 && words[index - 1].eq_ignore_ascii_case("delete");
        relation
            && !deleted
            && words
                .get(index + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case(table))
    })
}

/// The §4.2 filter: one `NOT EXISTS` over the retired-row tables the statement
/// must hide.
fn has_filter(sql: &str, markers: &[&str]) -> bool {
    let sql = without_comments(sql).to_ascii_lowercase();
    sql.contains("not exists") && markers.iter().all(|marker| sql.contains(marker))
}

/// One inline SQL literal found in the crate's own Rust source.
struct InlineSql {
    /// Path relative to the crate root, e.g. `src/backend/sqlite/heads.rs`.
    file: String,
    line: usize,
    sql: String,
}

/// An inline statement that reads a filtered table without the §4.2 filter,
/// and why. `needle` pins the statement: when the literal changes the entry
/// stops matching, so the test fails until the site is read again.
struct InlineAllow {
    file: &'static str,
    needle: &'static str,
    /// Retired-row tables the statement must still name, when it depends on
    /// seeing that state.
    markers: &'static [&'static str],
    reason: &'static str,
}

/// The inline read sites that may read `branch`/`layer_stack` unfiltered.
/// Every entry is a compaction-internal decision, not a convenience.
const INLINE_ALLOWED: &[InlineAllow] = &[
    InlineAllow {
        file: "src/backend/postgresql/upgrade/run.rs",
        needle: "FROM layer_stack),'')||'|'||coalesce((SELECT string_agg(encode(branch_id,'hex')",
        markers: &[],
        reason: "#804 store upgrade, message 2: the before/after heads digest covers every \
                 Branch and LayerStack row, a retired one included, because the step must \
                 prove it changed no row at all; it renders nothing and runs inside the \
                 upgrade's exclusive locks",
    },
    InlineAllow {
        file: "src/backend/postgresql/compact/cleanup.rs",
        needle: "UNION SELECT b.layer_stack_id FROM {schema}.branch b",
        markers: &[],
        reason: "5.1/8 compaction cleanup: the dead LayerStacks are the ones that carry \
                 `gc_dead` rows, which by construction are the rows the mark did not keep, \
                 so this read must see a retired Stack too; the ids are about to be deleted \
                 and nothing is rendered from this read",
    },
    InlineAllow {
        file: "src/backend/postgresql/compact/op.rs",
        needle: "SELECT 'branch',b.branch_id FROM {schema}.branch b",
        markers: &[],
        reason: "6.1/8 mark: the dead set is exactly the history the `kept_node` closure did \
                 not retain, a retired Branch's own row included, so the collection must not \
                 hide it; the statement writes `gc_dead`, renders nothing and runs inside the \
                 exclusive compaction window",
    },
    InlineAllow {
        file: "src/diagnostic/check/mounts.rs",
        needle: "SELECT g.op_id,g.scope_id,g.workspace,g.sandbox,g.machine",
        markers: &[],
        reason: "the mount_holders diagnostic is the read-only operator holder view; it must \
                 see a retired Branch/Stack so a holder is not hidden, and it changes nothing",
    },
    InlineAllow {
        file: "src/backend/sqlite/schema.rs",
        needle: "INSERT INTO project_epoch(layer_stack_id,epoch)",
        markers: &[],
        reason: "4/8 additive migration: a one-time seed of one `project_epoch` row per \
                 existing LayerStack, live or retired; not a client read (its PostgreSQL \
                 half was deleted with the PostgreSQL migrations, #776)",
    },
];

/// Every inline SQL literal in `src` that reads `branch` or `layer_stack`.
fn inline_sql() -> Vec<InlineSql> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_sources(&root, &mut files);
    let mut out = Vec::new();
    for path in files {
        let file = format!(
            "src/{}",
            path.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        );
        let source = fs::read_to_string(&path).unwrap();
        for (line, sql) in string_literals(&source) {
            if !sql.to_ascii_lowercase().contains("select") {
                continue;
            }
            if reads(&sql, "branch") || reads(&sql, "layer_stack") {
                out.push(InlineSql {
                    file: file.clone(),
                    line,
                    sql,
                });
            }
        }
    }
    out.sort_by(|left, right| {
        (left.file.as_str(), left.line).cmp(&(right.file.as_str(), right.line))
    });
    out
}

fn collect_sources(directory: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The `(line, text)` of every string literal in `source`, skipping comments.
/// Rust string escapes are flattened to the character they escape; a line
/// continuation contributes nothing, so a wrapped SQL statement reads as one
/// line of words.
fn string_literals(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut line = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let mut depth = 1usize;
                index += 2;
                while index < bytes.len() && depth > 0 {
                    match (bytes[index], bytes.get(index + 1)) {
                        (b'\n', _) => {
                            line += 1;
                            index += 1;
                        }
                        (b'/', Some(b'*')) => {
                            depth += 1;
                            index += 2;
                        }
                        (b'*', Some(b'/')) => {
                            depth -= 1;
                            index += 2;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'r' => {
                let mut cursor = index + 1;
                let mut hashes = 0usize;
                while bytes.get(cursor) == Some(&b'#') {
                    hashes += 1;
                    cursor += 1;
                }
                if bytes.get(cursor) != Some(&b'"') {
                    index += 1;
                    continue;
                }
                let start = line;
                cursor += 1;
                let mut text = String::new();
                while cursor < bytes.len() {
                    if bytes[cursor] == b'"' {
                        let mut close = cursor + 1;
                        let mut closing = 0usize;
                        while closing < hashes && bytes.get(close) == Some(&b'#') {
                            closing += 1;
                            close += 1;
                        }
                        if closing == hashes {
                            cursor = close;
                            break;
                        }
                    }
                    if bytes[cursor] == b'\n' {
                        line += 1;
                    }
                    text.push(bytes[cursor] as char);
                    cursor += 1;
                }
                out.push((start, text));
                index = cursor;
            }
            b'"' => {
                let start = line;
                let mut cursor = index + 1;
                let mut text = String::new();
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'\\' => {
                            cursor += 1;
                            if let Some(&next) = bytes.get(cursor) {
                                if next == b'\n' {
                                    line += 1;
                                } else {
                                    text.push(next as char);
                                }
                                cursor += 1;
                            }
                        }
                        b'"' => {
                            cursor += 1;
                            break;
                        }
                        b'\n' => {
                            line += 1;
                            text.push('\n');
                            cursor += 1;
                        }
                        other => {
                            text.push(other as char);
                            cursor += 1;
                        }
                    }
                }
                out.push((start, text));
                index = cursor;
            }
            _ => index += 1,
        }
    }
    out
}

#[test]
fn every_inline_sql_read_of_branch_or_layer_stack_is_filtered_or_allow_listed() {
    let statements = inline_sql();
    assert!(
        statements.len() >= 5,
        "the inline SQL scan found only {} reads; the scan boundary moved",
        statements.len()
    );
    let mut matched = BTreeSet::new();
    let mut violations = Vec::new();
    for statement in &statements {
        if let Some(allow) = INLINE_ALLOWED
            .iter()
            .find(|allow| allow.file == statement.file && statement.sql.contains(allow.needle))
        {
            assert!(
                !allow.reason.is_empty(),
                "{}: an inline allow-list entry needs a reason",
                statement.file
            );
            for marker in allow.markers {
                assert!(
                    statement.sql.contains(marker),
                    "{}:{} must still read {marker}",
                    statement.file,
                    statement.line
                );
            }
            matched.insert((allow.file, allow.needle));
            continue;
        }
        let branch = reads(&statement.sql, "branch");
        let markers: &[&str] = if branch {
            &["branch_retired", "layer_stack_retired"]
        } else {
            &["layer_stack_retired"]
        };
        if !has_filter(&statement.sql, markers) {
            violations.push(format!(
                "{}:{} reads `branch`/`layer_stack` without the §4.2 filter; add the \
                 NOT EXISTS markers or a reasoned inline allow-list entry: {}",
                statement.file,
                statement.line,
                statement.sql.trim()
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "inline SQL reads need the marker filter or an inline allow-list entry:\n\n{}",
        violations.join("\n\n")
    );
    for allow in INLINE_ALLOWED {
        assert!(
            matched.contains(&(allow.file, allow.needle)),
            "inline allow-list entry {} ({}) matches no inline read that reads branch/layer_stack",
            allow.file,
            allow.needle
        );
    }
}

#[test]
fn every_statement_reading_branch_or_layer_stack_is_filtered_or_allow_listed() {
    let statements = read_statements();
    assert!(
        statements.len() > 20,
        "the shipped statement set is unexpectedly small"
    );

    let mut reading = BTreeSet::new();
    let mut allowed = BTreeSet::new();
    for statement in &statements {
        let name = statement.file.as_str();
        let branch = reads(&statement.sql, "branch");
        let stack = reads(&statement.sql, "layer_stack");
        if !branch && !stack {
            continue;
        }
        reading.insert(name.to_owned());

        if let Some(allow) = ALLOWED.iter().find(|allow| allow.file == name) {
            allowed.insert(allow.file);
            assert!(
                !allow.reason.is_empty(),
                "{name}: an allow-list entry needs a reason"
            );
            for marker in allow.markers {
                assert!(
                    statement.sql.contains(marker),
                    "{name} ({}) must still read {marker}",
                    statement.backend
                );
            }
            if allow.returns_state {
                assert!(
                    statement.sql.to_ascii_uppercase().contains("CASE WHEN"),
                    "{name} ({}) must return the holder state",
                    statement.backend
                );
            }
            continue;
        }

        // A Branch belongs to a Stack, so hiding a deleted project needs the
        // stack marker even when the statement names only `branch`.
        let markers: &[&str] = if branch {
            &["branch_retired", "layer_stack_retired"]
        } else {
            &["layer_stack_retired"]
        };
        assert!(
            has_filter(&statement.sql, markers),
            "{} ({}) reads `branch`/`layer_stack` without the §4.2 filter; add the \
             NOT EXISTS markers or a reasoned allow-list entry: {}",
            name,
            statement.backend,
            statement.sql.trim()
        );
    }

    // The statements §4.2 names explicitly stay present, filtered, and out of
    // the allow-list; the scan alone would not notice a deletion or a
    // statement moving behind the allow-list.
    for name in BRANCH_FILTERED.iter().chain(STACK_FILTERED.iter()) {
        assert!(
            reading.contains(*name),
            "§4.2 statement {name} is not shipped"
        );
        assert!(
            !ALLOWED.iter().any(|allow| allow.file == *name),
            "{name} is a filtered statement and cannot be allow-listed"
        );
    }
    for allow in ALLOWED {
        assert!(
            allowed.contains(allow.file),
            "allow-list entry {} matches no statement that reads branch/layer_stack",
            allow.file
        );
    }
}

#[test]
fn the_guard_recognizes_a_new_unfiltered_statement() {
    let unfiltered = "SELECT branch_id FROM branch WHERE branch_id = $1";
    assert!(reads(unfiltered, "branch"));
    assert!(!has_filter(
        unfiltered,
        &["branch_retired", "layer_stack_retired"]
    ));

    let filtered = "SELECT branch_id FROM branch b WHERE NOT EXISTS (SELECT 1 FROM \
                    branch_retired r WHERE r.branch_id = b.branch_id) AND NOT EXISTS \
                    (SELECT 1 FROM layer_stack_retired s WHERE s.layer_stack_id = \
                    b.layer_stack_id)";
    assert!(has_filter(
        filtered,
        &["branch_retired", "layer_stack_retired"]
    ));

    // A write is not a read, and the marker table is not the base table.
    assert!(!reads("DELETE FROM branch WHERE branch_id = $1", "branch"));
    assert!(!reads(
        "UPDATE branch SET name = $1 WHERE branch_id = $2",
        "branch"
    ));
    assert!(!reads("SELECT branch_id FROM branch_retired", "branch"));
    assert!(reads(
        "SELECT layer_stack_id FROM {s}.layer_stack WHERE name = $1",
        "layer_stack"
    ));
}

#[test]
fn the_retry_stack_read_is_the_marker_plus_the_filtered_lookup() {
    // §4.2 names `layerstack_stack_by_id_retired`; the shipped retry splits
    // that read into the marker statement and the filtered by-id lookup, and
    // #42 A9 records the deviation. Pin both halves so neither drifts.
    let statements = read_statements();
    let find = |file: &str| {
        statements
            .iter()
            .find(|statement| statement.file == file)
            .unwrap_or_else(|| panic!("missing statement {file}"))
    };
    assert!(has_filter(
        &find("history/stack/by_id.sql").sql,
        &["layer_stack_retired"]
    ));
    assert!(
        find("history/stack/deleted_ms.sql")
            .sql
            .contains("layer_stack_retired"),
        "the retry's marker read must read layer_stack_retired"
    );
    assert!(
        !statements
            .iter()
            .any(|statement| statement.file == "history/stack/by_id_retired.sql"),
        "§4.2's named statement does not exist; #42 A9 records the split"
    );
}
