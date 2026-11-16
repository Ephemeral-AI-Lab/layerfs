//! §4.2 visibility guard: every shipped statement that reads `branch` or
//! `layer_stack` carries the retired-row `NOT EXISTS` filter unless it is on
//! the documented allow-list below.
//!
//! The statements are discovered from the shipped SQL directories rather than
//! from a hand-kept list, so a new statement that reads either table without
//! the filter fails here until it is filtered or allow-listed. Every allow-list
//! entry carries its reason and is proved to still be a live statement that
//! reads the table, so the list cannot rot. The lookup pairs and the
//! snapshot/live statements §4.2 names by hand are also pinned, because a scan
//! alone would not notice a deletion.

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
/// both snapshots and the GC live set.
const BRANCH_FILTERED: &[&str] = &[
    "branch_branch_by_id.sql",
    "branch_branch_by_name.sql",
    "branch_branch_first_page.sql",
    "branch_branch_next_page.sql",
    "branch_snapshot.sql",
    "branch_snapshot_named.sql",
    "gc_live_branches.sql",
];

/// The four project lookups, their live set and the PostgreSQL by-name
/// override (the reason the guard scans both backends).
const STACK_FILTERED: &[&str] = &[
    "layerstack_stack_by_id.sql",
    "layerstack_stack_by_name.sql",
    "layerstack_stack_first_page.sql",
    "layerstack_stack_next_page.sql",
    "gc_live_stacks.sql",
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
        file: "branch_branch_by_id_any.sql",
        markers: &[],
        returns_state: false,
        reason: "§4.2 render/identity for layer.list, layer.get, saved and operator \
                 inspect: a collected Branch's own row is still rendered, so this by-id \
                 read is deliberately unfiltered; the filtered branch_branch_by_id is \
                 the lookup path",
    },
    Allow {
        file: "branch_branch_name_taken.sql",
        markers: &["branch_retired"],
        returns_state: true,
        reason: "§4.2 name-collision check: it must see the retired holder and return \
                 its state so branch.fork answers NameTaken with the holder",
    },
    Allow {
        file: "gc_branch_stack_any.sql",
        markers: &[],
        returns_state: false,
        reason: "§5.1/§8 compaction admission: maps a Branch-scoped intent to its Stack \
                 for the PostgreSQL advisory release keys, including a Branch retired \
                 while the intent was open; the id is already recorded and nothing is \
                 rendered from this read",
    },
    Allow {
        file: "gc_intent_all_project.sql",
        markers: &[],
        returns_state: false,
        reason: "§5.1/§8 compaction admission: collects every open intent for a project, \
                 including intents scoped to its retired Branches, so a delete or \
                 compaction cannot leak one",
    },
    Allow {
        file: "gc_intent_ops_project.sql",
        markers: &[],
        returns_state: false,
        reason: "§5.1/§8 compaction admission: the removal/compaction subset of \
                 gc_intent_all_project, with the same retired-Branch coverage",
    },
    Allow {
        file: "history_remove_unreachable_reach.sql",
        markers: &[],
        returns_state: false,
        reason: "§6.1/§7 mark cleanup: deletes `reach` rows whose key is no longer a base \
                 `layer_stack` row; a retired row is still a base row until the rebuild \
                 collects it, so this is not a visibility lookup",
    },
    Allow {
        file: "history_unreachable_count.sql",
        markers: &[],
        returns_state: false,
        reason: "§6.1/§7 mark: counts the unretained Commit/Layer/Branch/Stack rows a \
                 rebuild collects, and the retired rows are exactly what it must count; \
                 the retired markers would hide them from the count",
    },
    Allow {
        file: "layerstack_stack_head.sql",
        markers: &[],
        returns_state: false,
        reason: "§4 add_layer: the StackMoved report reads the actual head after the \
                 filtered stack lookup already resolved a live Stack under the Store \
                 write lane; filtering would turn StackMoved into Integrity",
    },
    Allow {
        file: "layerstack_stack_name_taken.sql",
        markers: &["layer_stack_retired"],
        returns_state: true,
        reason: "§4.2 project name-collision check: it must see the deleted holder and \
                 return its state so project.init answers NameTaken with the holder",
    },
];

fn read_statements() -> Vec<Statement> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("sql");
    let mut statements = Vec::new();
    for backend in ["sqlite", "postgresql"] {
        let directory = root.join(backend).join("queries").join("history");
        let entries = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("sql") {
                continue;
            }
            let file = path.file_name().unwrap().to_str().unwrap().to_owned();
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

/// Whether `sql` reads rows of `table` through a `FROM`/`JOIN` relation
/// (including a subquery), rather than writing to it. `DELETE FROM` and
/// an `UPDATE`/`INSERT` are not reads; `INSERT ... SELECT ... FROM` is.
fn reads(sql: &str, table: &str) -> bool {
    let normalized = without_comments(sql).replace("{s}.", "");
    let tokens = words(&normalized);
    tokens.iter().enumerate().any(|(index, token)| {
        let relation = token.eq_ignore_ascii_case("from") || token.eq_ignore_ascii_case("join");
        let deleted = index > 0 && tokens[index - 1].eq_ignore_ascii_case("delete");
        relation
            && !deleted
            && tokens
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
            "{name} ({}) reads `branch`/`layer_stack` without the §4.2 filter; add the \
             NOT EXISTS markers or a reasoned allow-list entry: {}",
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
        &find("layerstack_stack_by_id.sql").sql,
        &["layer_stack_retired"]
    ));
    assert!(
        find("layerstack_deleted_ms.sql")
            .sql
            .contains("layer_stack_retired"),
        "the retry's marker read must read layer_stack_retired"
    );
    assert!(
        !statements
            .iter()
            .any(|statement| statement.file == "layerstack_stack_by_id_retired.sql"),
        "§4.2's named statement does not exist; #42 A9 records the split"
    );
}
