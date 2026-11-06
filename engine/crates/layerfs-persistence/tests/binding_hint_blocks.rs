//! Binding hints in append-only blocks (#91): each write appends blocks of
//! at most 4,000 bytes per group of 64 serials, a reader merges a group's
//! blocks and drops repeats, and a hint no table row could hold is refused
//! before anything is written.
mod support;
use layerfs_history::{records::BindingHint, HistoryCatalog};
use support::*;

fn hint(child: u64, parent: u64, name: &str) -> BindingHint {
    BindingHint {
        child,
        parent,
        name: name.as_bytes().to_vec(),
    }
}

fn blocks(path: &std::path::Path) -> Vec<(i64, i64, usize)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    let mut statement = sql
        .prepare("SELECT grp,seq,length(body) FROM binding_hint_block ORDER BY grp,seq")
        .unwrap();
    statement
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as usize))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn hints_are_merged_across_blocks_without_repeats() {
    let t = Temp::new("hint-blocks");
    let path = t.join("db");
    let catalog = create(&path);
    // Group 0 holds serials 1..=63; serial 64 starts group 1.
    let mut first = vec![
        hint(7, 1, "b.txt"),
        hint(7, 1, "a.txt"),
        hint(7, 3, "a.txt"),
        hint(63, 1, "last"),
        hint(64, 1, "next-group"),
        hint(7, 1, "a.txt"),
    ];
    // Long names that share no prefix pass one block: the write appends
    // several of one group.
    for index in 0..40u64 {
        first.push(hint(8 + index, 2, &format!("{index}-{}", "x".repeat(200))));
    }
    catalog.record_bindings(&first).unwrap();
    let written = blocks(&path);
    assert!(written.iter().all(|b| b.2 <= 4_000), "{written:?}");
    assert!(
        written.iter().filter(|b| b.0 == 0).count() >= 2,
        "{written:?}"
    );
    assert_eq!(written.iter().filter(|b| b.0 == 1).count(), 1);
    // A second write repeats one hint and adds one: a new block, no change
    // to the stored ones, and the reader shows each binding once.
    catalog
        .record_bindings(&[hint(7, 1, "a.txt"), hint(7, 2, "c.txt")])
        .unwrap();
    let after = blocks(&path);
    assert_eq!(after.len(), written.len() + 1);
    assert!(written.iter().all(|block| after.contains(block)));
    assert_eq!(
        catalog.bindings_of(7).unwrap(),
        vec![
            hint(7, 1, "a.txt"),
            hint(7, 1, "b.txt"),
            hint(7, 2, "c.txt"),
            hint(7, 3, "a.txt"),
        ]
    );
    assert_eq!(catalog.bindings_of(63).unwrap(), vec![hint(63, 1, "last")]);
    assert_eq!(
        catalog.bindings_of(64).unwrap(),
        vec![hint(64, 1, "next-group")]
    );
    assert_eq!(catalog.bindings_of(47).unwrap().len(), 1);
    assert!(catalog.bindings_of(65).unwrap().is_empty());
    assert!(catalog.bindings_of(1 << 40).unwrap().is_empty());
}

#[test]
fn a_hint_no_row_could_hold_is_refused_and_nothing_is_written() {
    let t = Temp::new("hint-blocks-refused");
    let path = t.join("db");
    let catalog = create(&path);
    for bad in [
        hint(0, 1, "a"),
        hint(1, 0, "a"),
        hint(1, 1, ""),
        BindingHint {
            child: 1,
            parent: 1,
            name: vec![b'n'; 256],
        },
    ] {
        assert!(catalog.record_bindings(&[hint(2, 1, "fine"), bad]).is_err());
    }
    assert!(blocks(&path).is_empty());
    assert!(catalog.bindings_of(2).unwrap().is_empty());
}

#[test]
fn a_changed_block_fails_the_read_of_its_group() {
    let t = Temp::new("hint-blocks-corrupt");
    let path = t.join("db");
    let catalog = create(&path);
    catalog.record_bindings(&[hint(5, 1, "name")]).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    let mut body: Vec<u8> = sql
        .query_row("SELECT body FROM binding_hint_block", [], |r| r.get(0))
        .unwrap();
    body[3] ^= 0x10;
    sql.execute("UPDATE binding_hint_block SET body=?1", [body])
        .unwrap();
    assert!(catalog.bindings_of(5).is_err());
    assert!(catalog.bindings_of(64).unwrap().is_empty());
}

/// The blocks of four fixed hints, byte for byte (see
/// `tools/test_store_blocks.py`).
#[test]
fn the_block_format_is_the_documented_one() {
    let t = Temp::new("hint-blocks-golden");
    let path = t.join("db");
    let catalog = create(&path);
    catalog
        .record_bindings(&[
            hint(7, 1, "a.txt"),
            hint(7, 1, "b.txt"),
            hint(9, 2, "a.txt"),
            hint(70, 1, "x"),
        ])
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    let mut statement = sql
        .prepare("SELECT grp,seq,body FROM binding_hint_block ORDER BY grp")
        .unwrap();
    let rows: Vec<(i64, i64, Vec<u8>)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let unhex = |hex: &str| -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    };
    assert_eq!(
        rows,
        vec![
            (
                0,
                1,
                unhex("010307020005612e74787400000005622e74787402020005612e7478742d1e97ab")
            ),
            (1, 1, unhex("0101060200017860fbeefb")),
        ]
    );
}
