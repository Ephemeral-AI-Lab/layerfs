pub const ALL: &[(&str, &str)] = &[
    ("schema/v4.sql", schema::V4),
    ("schema/v5.sql", schema::V5),
    ("schema/migrate_v4_to_v5.sql", schema::MIGRATE_V4_TO_V5),
    ("schema/schema_objects.sql", schema::SCHEMA_OBJECTS),
    ("schema/table_columns.sql", schema::TABLE_COLUMNS),
    ("schema/foreign_key_check.sql", schema::FOREIGN_KEY_CHECK),
    ("objects/get.sql", objects::GET),
    ("objects/get_many_128.sql", objects::GET_MANY_128),
    ("objects/membership_128.sql", objects::MEMBERSHIP_128),
    ("objects/insert.sql", objects::INSERT),
    ("objects/equal.sql", objects::EQUAL),
    ("objects/page.sql", objects::PAGE),
    ("layerstack/get.sql", layerstack::GET),
    ("layerstack/get_by_name.sql", layerstack::GET_BY_NAME),
    ("layerstack/list.sql", layerstack::LIST),
    ("layerstack/insert.sql", layerstack::INSERT),
    ("layerstack/get_layer.sql", layerstack::GET_LAYER),
    ("layerstack/list_layers.sql", layerstack::LIST_LAYERS),
    ("layerstack/insert_layer.sql", layerstack::INSERT_LAYER),
    (
        "layerstack/find_layer_by_source.sql",
        layerstack::FIND_LAYER_BY_SOURCE,
    ),
    (
        "layerstack/load_add_snapshot.sql",
        layerstack::LOAD_ADD_SNAPSHOT,
    ),
    ("layerstack/advance_head.sql", layerstack::ADVANCE_HEAD),
    ("layerstack/current_head.sql", layerstack::CURRENT_HEAD),
    ("layerstack/history_page.sql", layerstack::HISTORY_PAGE),
    ("branch/get.sql", branch::GET),
    ("branch/get_by_name.sql", branch::GET_BY_NAME),
    ("branch/list.sql", branch::LIST),
    ("branch/insert.sql", branch::INSERT),
    ("branch/get_commit.sql", branch::GET_COMMIT),
    ("branch/list_commits.sql", branch::LIST_COMMITS),
    ("branch/history_page.sql", branch::HISTORY_PAGE),
    ("branch/contains_commit.sql", branch::CONTAINS_COMMIT),
    ("workspace/load_snapshot.sql", workspace::LOAD_SNAPSHOT),
    ("workspace/insert_commit.sql", workspace::INSERT_COMMIT),
    ("workspace/advance_branch.sql", workspace::ADVANCE_BRANCH),
    ("workspace/current_branch.sql", workspace::CURRENT_BRANCH),
    ("workspace/insert_stage.sql", workspace::INSERT_STAGE),
    ("workspace/get_stage.sql", workspace::GET_STAGE),
    ("workspace/delete_stage.sql", workspace::DELETE_STAGE),
    ("query/store_counts.sql", query::STORE_COUNTS),
    ("query/canonical_storage.sql", query::CANONICAL_STORAGE),
    ("query/layer_roots_page.sql", query::LAYER_ROOTS_PAGE),
    ("query/commit_roots_page.sql", query::COMMIT_ROOTS_PAGE),
    ("query/branch_roots_page.sql", query::BRANCH_ROOTS_PAGE),
];

pub mod schema {
    pub const V4: &str = include_str!("../sql/schema/v4.sql");
    pub const V5: &str = include_str!("../sql/schema/v5.sql");
    pub const MIGRATE_V4_TO_V5: &str = include_str!("../sql/schema/migrate_v4_to_v5.sql");
    pub const SCHEMA_OBJECTS: &str = include_str!("../sql/schema/schema_objects.sql");
    pub const TABLE_COLUMNS: &str = include_str!("../sql/schema/table_columns.sql");
    pub const FOREIGN_KEY_CHECK: &str = include_str!("../sql/schema/foreign_key_check.sql");
}

pub mod objects {
    pub const GET: &str = include_str!("../sql/objects/get.sql");
    pub const GET_MANY_128: &str = include_str!("../sql/objects/get_many_128.sql");
    pub const MEMBERSHIP_128: &str = include_str!("../sql/objects/membership_128.sql");
    pub const INSERT: &str = include_str!("../sql/objects/insert.sql");
    pub const EQUAL: &str = include_str!("../sql/objects/equal.sql");
    pub const PAGE: &str = include_str!("../sql/objects/page.sql");
}

pub mod layerstack {
    pub const GET: &str = include_str!("../sql/layerstack/get.sql");
    pub const GET_BY_NAME: &str = include_str!("../sql/layerstack/get_by_name.sql");
    pub const LIST: &str = include_str!("../sql/layerstack/list.sql");
    pub const INSERT: &str = include_str!("../sql/layerstack/insert.sql");
    pub const GET_LAYER: &str = include_str!("../sql/layerstack/get_layer.sql");
    pub const LIST_LAYERS: &str = include_str!("../sql/layerstack/list_layers.sql");
    pub const INSERT_LAYER: &str = include_str!("../sql/layerstack/insert_layer.sql");
    pub const FIND_LAYER_BY_SOURCE: &str =
        include_str!("../sql/layerstack/find_layer_by_source.sql");
    pub const LOAD_ADD_SNAPSHOT: &str = include_str!("../sql/layerstack/load_add_snapshot.sql");
    pub const ADVANCE_HEAD: &str = include_str!("../sql/layerstack/advance_head.sql");
    pub const CURRENT_HEAD: &str = include_str!("../sql/layerstack/current_head.sql");
    pub const HISTORY_PAGE: &str = include_str!("../sql/layerstack/history_page.sql");
}

pub mod branch {
    pub const GET: &str = include_str!("../sql/branch/get.sql");
    pub const GET_BY_NAME: &str = include_str!("../sql/branch/get_by_name.sql");
    pub const LIST: &str = include_str!("../sql/branch/list.sql");
    pub const INSERT: &str = include_str!("../sql/branch/insert.sql");
    pub const GET_COMMIT: &str = include_str!("../sql/branch/get_commit.sql");
    pub const LIST_COMMITS: &str = include_str!("../sql/branch/list_commits.sql");
    pub const HISTORY_PAGE: &str = include_str!("../sql/branch/history_page.sql");
    pub const CONTAINS_COMMIT: &str = include_str!("../sql/branch/contains_commit.sql");
}

pub mod workspace {
    pub const LOAD_SNAPSHOT: &str = include_str!("../sql/workspace/load_snapshot.sql");
    pub const INSERT_COMMIT: &str = include_str!("../sql/workspace/insert_commit.sql");
    pub const ADVANCE_BRANCH: &str = include_str!("../sql/workspace/advance_branch.sql");
    pub const CURRENT_BRANCH: &str = include_str!("../sql/workspace/current_branch.sql");
    pub const INSERT_STAGE: &str = include_str!("../sql/workspace/insert_stage.sql");
    pub const GET_STAGE: &str = include_str!("../sql/workspace/get_stage.sql");
    pub const DELETE_STAGE: &str = include_str!("../sql/workspace/delete_stage.sql");
}

pub mod query {
    pub const STORE_COUNTS: &str = include_str!("../sql/query/store_counts.sql");
    pub const CANONICAL_STORAGE: &str = include_str!("../sql/query/canonical_storage.sql");
    pub const LAYER_ROOTS_PAGE: &str = include_str!("../sql/query/layer_roots_page.sql");
    pub const COMMIT_ROOTS_PAGE: &str = include_str!("../sql/query/commit_roots_page.sql");
    pub const BRANCH_ROOTS_PAGE: &str = include_str!("../sql/query/branch_roots_page.sql");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params_from_iter, types::Value, Connection};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn exact_manifest_prepares_against_exact_v5_schema() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sql");
        let mut files = Vec::new();
        collect_sql(&root, &root, &mut files);
        files.sort();
        let mut registered = ALL
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect::<Vec<_>>();
        registered.sort();
        assert_eq!(files, registered);
        assert_eq!(ALL.len(), 44);

        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection.execute_batch(schema::V5).unwrap();

        let expected_parameters = BTreeMap::from([
            ("schema/schema_objects.sql", 0),
            ("schema/table_columns.sql", 1),
            ("schema/foreign_key_check.sql", 0),
            ("objects/get.sql", 1),
            ("objects/get_many_128.sql", 128),
            ("objects/membership_128.sql", 128),
            ("objects/insert.sql", 2),
            ("objects/equal.sql", 2),
            ("objects/page.sql", 2),
            ("layerstack/get.sql", 1),
            ("layerstack/get_by_name.sql", 1),
            ("layerstack/list.sql", 2),
            ("layerstack/insert.sql", 3),
            ("layerstack/get_layer.sql", 1),
            ("layerstack/list_layers.sql", 3),
            ("layerstack/insert_layer.sql", 6),
            ("layerstack/find_layer_by_source.sql", 2),
            ("layerstack/load_add_snapshot.sql", 1),
            ("layerstack/advance_head.sql", 3),
            ("layerstack/current_head.sql", 1),
            ("layerstack/history_page.sql", 2),
            ("branch/get.sql", 1),
            ("branch/get_by_name.sql", 2),
            ("branch/list.sql", 3),
            ("branch/insert.sql", 5),
            ("branch/get_commit.sql", 1),
            ("branch/list_commits.sql", 2),
            ("branch/history_page.sql", 2),
            ("branch/contains_commit.sql", 3),
            ("workspace/load_snapshot.sql", 1),
            ("workspace/insert_commit.sql", 4),
            ("workspace/advance_branch.sql", 5),
            ("workspace/current_branch.sql", 1),
            ("workspace/insert_stage.sql", 3),
            ("workspace/get_stage.sql", 1),
            ("workspace/delete_stage.sql", 3),
            ("query/store_counts.sql", 0),
            ("query/canonical_storage.sql", 0),
            ("query/layer_roots_page.sql", 2),
            ("query/commit_roots_page.sql", 2),
            ("query/branch_roots_page.sql", 2),
        ]);

        for (name, sql) in ALL.iter().filter(|(name, _)| {
            !matches!(
                *name,
                "schema/v4.sql" | "schema/v5.sql" | "schema/migrate_v4_to_v5.sql"
            )
        }) {
            assert!(sql.starts_with("-- family:"), "missing header: {name}");
            assert!(sql.contains("\n-- name:"), "missing name header: {name}");
            assert!(
                sql.contains("\n-- parameters:"),
                "missing parameter header: {name}"
            );
            assert_eq!(sql.matches(';').count(), 1, "not one statement: {name}");
            let statement = connection
                .prepare(sql)
                .unwrap_or_else(|error| panic!("failed to prepare {name}: {error}"));
            assert_eq!(
                statement.parameter_count(),
                expected_parameters[name],
                "parameter count: {name}"
            );
        }

        let application_id: i64 = connection
            .query_row("PRAGMA application_id", [], |row| row.get(0))
            .unwrap();
        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(application_id, 0x4c46_534c);
        assert_eq!(user_version, 5);

        let tables = connection
            .prepare(
                "SELECT name,ncol,wr,strict FROM pragma_table_list \
                 WHERE schema='main' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            tables,
            vec![
                ("branches".to_owned(), 5, 1, 1),
                ("commits".to_owned(), 4, 1, 1),
                ("layer_stacks".to_owned(), 3, 1, 1),
                ("layers".to_owned(), 6, 1, 1),
                ("objects".to_owned(), 2, 0, 1),
                ("workspace_stages".to_owned(), 3, 1, 1),
            ]
        );
        assert_eq!(tables.iter().map(|table| table.1).sum::<i64>(), 23);

        let indexes = connection
            .prepare(
                "SELECT name FROM sqlite_schema \
                 WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<BTreeSet<_>>>()
            .unwrap();
        assert_eq!(
            indexes,
            BTreeSet::from([
                "branch_identity".to_owned(),
                "branch_names".to_owned(),
                "layer_identity".to_owned(),
                "layer_stack_names".to_owned(),
                "layers_child".to_owned(),
                "layers_genesis".to_owned(),
                "layers_source".to_owned(),
            ])
        );
    }

    #[test]
    fn point_name_and_keyset_queries_use_indexed_search_plans() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(schema::V4).unwrap();
        let id17 = Value::Blob(vec![0; 17]);
        let id32 = Value::Blob(vec![0; 32]);
        let id33 = Value::Blob(vec![0; 33]);
        for (name, sql, parameters) in [
            ("object point", objects::GET, vec![id32.clone()]),
            (
                "object keyset",
                objects::PAGE,
                vec![id32, Value::Integer(128)],
            ),
            ("LayerStack point", layerstack::GET, vec![id17.clone()]),
            (
                "LayerStack name",
                layerstack::GET_BY_NAME,
                vec![Value::Text("demo".to_owned())],
            ),
            (
                "LayerStack keyset",
                layerstack::LIST,
                vec![id17.clone(), Value::Integer(128)],
            ),
            ("Branch point", branch::GET, vec![id17.clone()]),
            (
                "Branch name",
                branch::GET_BY_NAME,
                vec![id17.clone(), Value::Text("main".to_owned())],
            ),
            (
                "Branch keyset",
                branch::LIST,
                vec![id17.clone(), id17.clone(), Value::Integer(128)],
            ),
            (
                "Layer keyset",
                layerstack::LIST_LAYERS,
                vec![id17, id33, Value::Integer(128)],
            ),
        ] {
            let explain = format!("EXPLAIN QUERY PLAN {sql}");
            let plan = connection
                .prepare(&explain)
                .unwrap()
                .query_map(params_from_iter(parameters), |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert!(
                plan.iter().any(|detail| detail.contains("SEARCH")),
                "{name} did not SEARCH: {plan:?}"
            );
        }
    }

    fn collect_sql(root: &std::path::Path, path: &std::path::Path, files: &mut Vec<String>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_sql(root, &path, files);
            } else if path.extension().and_then(std::ffi::OsStr::to_str) == Some("sql") {
                files.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
}
