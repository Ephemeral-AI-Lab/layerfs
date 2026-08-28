use super::super::artifact::unix_ns;
use super::super::limits::INITIAL_BYTES;
use super::super::receipt_model::{OracleReceipt, ResourceObservation, RowReceipt};
use super::super::resources::unavailable_defaults;
use super::super::row_parse::parse_rows;
use super::super::schedule::frozen_schedule;
use super::super::summary_json_parse::json_top_level_value;
use super::synthetic::synthetic_pass_row;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn row_contract_is_valid_json_and_retains_null_unavailable_observations() {
    let schedule = frozen_schedule().unwrap();
    let row = RowReceipt {
        schedule: schedule.rows[0].clone(),
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: Vec::new(),
        phase_counters: Vec::new(),
        row_wall_ns: 0,
        row_residual_ns: 0,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources: ResourceObservation::default(),
        oracle: OracleReceipt::default(),
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    }
    .json()
    .unwrap();
    assert!(row.contains("\"rollback_journal_bytes\":null"));
    assert!(row.contains("\"availability\":\"Unavailable\""));
    assert!(row.contains("\"sync_regular_calls\":null"));
    assert!(row.contains("\"transactions_started\":null"));
    assert!(row.contains("\"availability\":\"NotApplicable\""));
    assert!(row.contains("\"field\":\"oracle.physical_bytes_exact\""));
    let mut child = Command::new("/usr/bin/ruby")
        .args(["-rjson", "-e", "JSON.parse(STDIN.read)"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(row.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "row={} stdout={} stderr={}",
        row,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn all_47_common_rows_round_trip_in_frozen_order() {
    let schedule = frozen_schedule().unwrap();
    let path = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-row-contract-{}-{}.jsonl",
        std::process::id(),
        unix_ns().unwrap()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    for scheduled in &schedule.rows {
        file.write_all(
            synthetic_pass_row(&schedule, scheduled)
                .json()
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    }
    drop(file);
    let parsed = parse_rows(&path, &schedule).unwrap();
    assert_eq!(parsed.len(), 47);
    assert_eq!(parsed[8].row_id, "C04-001");
    assert_eq!(parsed[9].row_id, "C03-006");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        contents.replacen("\"row_group\":\"C00\"", "\"row_group\":\"C01\"", 1),
    )
    .unwrap();
    assert!(parse_rows(&path, &schedule).is_err());
    let c07 = contents
        .lines()
        .find(|line| line.contains("\"row_id\":\"C07-001\""))
        .unwrap();
    for key in [
        "before_bytes",
        "after_bytes",
        "native_route",
        "tree_level_before",
    ] {
        let value = json_top_level_value(c07, key).unwrap();
        let value_offset = c07.len() - value.len();
        let key_start = c07[..value_offset].rfind(&format!("\"{key}\":")).unwrap() + 1;
        let mut mutated = c07.to_owned();
        mutated.replace_range(key_start..key_start + key.len(), &format!("removed_{key}"));
        fs::write(&path, contents.replacen(c07, &mutated, 1)).unwrap();
        assert!(parse_rows(&path, &schedule).is_err(), "top-level {key}");
    }
    fs::remove_file(path).unwrap();
}
