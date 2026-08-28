use super::super::limits::{BUFFER_BYTES, INITIAL_BYTES, MAXIMUM_BYTES};
use super::super::oracle::PieceCursor;
use super::super::schedule::{frozen_schedule, oracle_snapshots};
use super::super::schedule_model::{EditKind, EditSpec, Piece, PieceTable};
use super::super::source_identity::schedule_json;
use crate::stage1_fixture;

#[test]
fn schedule_and_piece_table_close_the_frozen_population() {
    let schedule = frozen_schedule().unwrap();
    let snapshots = oracle_snapshots(&schedule).unwrap();
    assert_eq!(schedule.rows.len(), 47);
    assert_eq!(schedule.edits.len(), 51);
    assert_eq!(snapshots.len(), 35);
    assert_eq!(snapshots[15].logical_length, INITIAL_BYTES);
    assert_eq!(snapshots[30].logical_length, INITIAL_BYTES);
    assert_eq!(snapshots[34].logical_length, INITIAL_BYTES);
    assert_eq!(schedule.replacement_backing.len(), 495_616);
    assert_eq!(
        schedule.edits.iter().map(|edit| edit.after_bytes).max(),
        Some(MAXIMUM_BYTES)
    );
}
#[test]
fn piece_table_matches_a_reduced_vec_after_every_splice() {
    let mut table = PieceTable {
        pieces: vec![Piece::Inserted {
            offset: 0,
            length: 32,
        }],
        logical_length: 32,
    };
    let mut backing = (0_u8..64).collect::<Vec<_>>();
    let mut expected = backing[..32].to_vec();
    let edits = [
        ("t1", 4, 3, 5, 32, 34, 32),
        ("t2", 0, 0, 2, 34, 36, 37),
        ("t3", 30, 6, 0, 36, 30, 39),
    ];
    for (serial, &(tag, offset, delete, insert, before, after, replacement_offset)) in
        edits.iter().enumerate()
    {
        let edit = EditSpec {
            tag: tag.to_owned(),
            serial: serial as u8,
            epoch: 0,
            kind: EditKind::Overwrite,
            size_band: "test",
            offset,
            delete_bytes: delete,
            insert_bytes: insert,
            before_bytes: before,
            after_bytes: after,
            replacement_offset,
        };
        table.splice(&edit).unwrap();
        let replacement = backing
            [replacement_offset..replacement_offset + usize::try_from(insert).unwrap()]
            .to_vec();
        expected.splice(
            usize::try_from(offset).unwrap()..usize::try_from(offset + delete).unwrap(),
            replacement,
        );
        let mut actual = Vec::new();
        table.stream(&backing, &mut actual).unwrap();
        assert_eq!(actual, expected);
    }
    backing.clear();
}
#[test]
fn piece_cursor_generates_each_original_mebibyte_once() {
    let table = PieceTable {
        pieces: vec![Piece::Original {
            offset: 0,
            length: BUFFER_BYTES as u64 + 1,
        }],
        logical_length: BUFFER_BYTES as u64 + 1,
    };
    let mut cursor = PieceCursor::new(&table, &[]);
    let mut chunk = vec![0_u8; 4_096];
    let mut expected = vec![0_u8; BUFFER_BYTES];
    stage1_fixture::fill_retained_buffer(&mut expected, 0);
    for index in 0..BUFFER_BYTES / chunk.len() {
        cursor.read_exact_expected(&mut chunk).unwrap();
        assert_eq!(
            chunk,
            expected[index * chunk.len()..(index + 1) * chunk.len()]
        );
    }
    assert_eq!(cursor.original_blocks_generated, 1);
    cursor.read_exact_expected(&mut chunk[..1]).unwrap();
    stage1_fixture::fill_retained_buffer(&mut expected, BUFFER_BYTES as u64);
    assert_eq!(chunk[0], expected[0]);
    assert_eq!(cursor.original_blocks_generated, 2);
    cursor.finish().unwrap();
}
#[test]
fn schedule_json_retains_every_edit_and_row_in_execution_order() {
    let schedule = frozen_schedule().unwrap();
    let json = schedule_json(&schedule).unwrap();
    assert_eq!(json.matches("\"row_id\":").count(), 47);
    assert_eq!(json.matches("\"tag\":").count(), 51);
    assert!(json.find("C03-005").unwrap() < json.find("C04-001").unwrap());
    assert!(json.find("C04-001").unwrap() < json.find("C03-006").unwrap());
    assert_eq!(json.matches("\"pre_ref_slot\":\"R").count(), 34);
    assert_eq!(json.matches("\"post_ref_slot\":\"R").count(), 34);
    assert!(json.contains("\"pre_ref_slot\":\"R0\",\"post_ref_slot\":\"R1\""));
    assert!(json.contains("\"pre_ref_slot\":\"R33\",\"post_ref_slot\":\"R34\""));
}
#[test]
#[ignore = "full 24 MiB x 51 exact differential proof; run once at source closure"]
fn all_51_exact_edits_match_the_independent_vec_digest_after_every_operation() {
    let schedule = frozen_schedule().unwrap();
    let mut expected = Vec::with_capacity(MAXIMUM_BYTES as usize);
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut offset = 0_u64;
    while offset < INITIAL_BYTES {
        stage1_fixture::fill_retained_buffer(&mut buffer, offset);
        let take = usize::try_from((INITIAL_BYTES - offset).min(BUFFER_BYTES as u64)).unwrap();
        expected.extend_from_slice(&buffer[..take]);
        offset += take as u64;
    }
    let mut table = PieceTable::initial();
    for edit in &schedule.edits {
        let start = usize::try_from(edit.offset).unwrap();
        let end = usize::try_from(edit.offset + edit.delete_bytes).unwrap();
        let replacement_end = edit.replacement_offset + usize::try_from(edit.insert_bytes).unwrap();
        expected.splice(
            start..end,
            schedule.replacement_backing[edit.replacement_offset..replacement_end]
                .iter()
                .copied(),
        );
        table.splice(edit).unwrap();
        assert_eq!(expected.len() as u64, edit.after_bytes, "{}", edit.tag);
        let mut comparison = PieceCursor::new(&table, &schedule.replacement_backing);
        let mut actual = vec![0_u8; BUFFER_BYTES];
        for chunk in expected.chunks(BUFFER_BYTES) {
            comparison
                .read_exact_expected(&mut actual[..chunk.len()])
                .unwrap();
            assert_eq!(&actual[..chunk.len()], chunk, "{}", edit.tag);
        }
        comparison.finish().unwrap();
    }
}
