use super::*;
use crate::sqlite::admission::index::validate_index_schemas;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

pub(super) fn create_contract(contract: SchemaContract) -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    for partition in contract.table_partitions {
        for (_, sql) in *partition {
            connection.execute_batch(sql).unwrap();
        }
    }
    for (_, sql) in contract.index_schemas {
        connection.execute_batch(sql).unwrap();
    }
    connection
}

fn table_names(connection: &Connection) -> Vec<String> {
    connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT GLOB 'sqlite_*' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

pub(super) fn columns(connection: &Connection, table: &str) -> Vec<String> {
    connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

pub(super) fn assert_sql_shapes(connection: &Connection, contract: SchemaContract) {
    let mut declared = Vec::new();
    for partition in contract.table_partitions {
        for (name, expected) in *partition {
            declared.push(*name);
            let actual = connection
                .query_row(
                    "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                    params![name],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
            assert_eq!(schema_shape(&actual), schema_shape(expected), "{name}");
            assert!(!columns(connection, name).is_empty(), "{name}");
        }
    }
    declared.sort_unstable();
    assert_eq!(declared, contract.table_names);
    assert_eq!(table_names(connection), contract.table_names);
    validate_index_schemas(connection, contract.index_schemas).unwrap();
}

pub(super) fn assert_fk_targets_are_local(
    connection: &Connection,
    contract: SchemaContract,
) -> usize {
    let names = contract
        .table_names
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut count = 0;
    for table in contract.table_names {
        let targets = connection
            .prepare(&format!("PRAGMA foreign_key_list({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(2))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for target in targets {
            assert!(names.contains(target.as_str()), "{table} -> {target}");
            count += 1;
        }
    }
    let violation = connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()
        .unwrap();
    assert!(violation.is_none());
    count
}

pub(super) fn query_plan(connection: &Connection, sql: &str) -> String {
    connection
        .prepare(sql)
        .unwrap()
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n")
}
