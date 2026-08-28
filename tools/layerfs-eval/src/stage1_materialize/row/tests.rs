use super::projection::delta;
use super::run::merge_terminal_cleanup;
use crate::legacy_full::OperationDiagnostics;
use crate::stage1_materialize::attribution::projection::scratch_sql;

#[test]
fn delta_rejects_backwards_counters() {
    assert_eq!(delta(9, 4, "x").unwrap(), 5);
    assert!(delta(4, 9, "x").is_err());
}

#[test]
fn terminal_scratch_cleanup_closes_the_complete_row_sql_equation() {
    let operation = OperationDiagnostics {
        scratch_tables: 1,
        scratch_statements: 19,
        scratch_owner_setup_statements: 15,
        scratch_derived_setup_statements: 2,
        scratch_operation_statements: 2,
        ..OperationDiagnostics::default()
    };
    let cleanup = OperationDiagnostics {
        scratch_statements: 1,
        scratch_derived_setup_statements: 1,
        ..OperationDiagnostics::default()
    };
    let terminal = merge_terminal_cleanup(operation, cleanup).unwrap();
    assert_eq!(terminal.scratch_tables, 1);
    assert_eq!(terminal.scratch_statements, 20);
    assert_eq!(terminal.scratch_owner_setup_statements, 15);
    assert_eq!(terminal.scratch_derived_setup_statements, 3);
    assert_eq!(terminal.scratch_operation_statements, 2);
    assert_eq!(scratch_sql(&terminal).unwrap(), terminal.scratch_statements);
}
