use super::super::*;
use super::fixture::assert_default_cache_budget;
use crate::scratch::schema::DISK_TABLE_CACHE_KIB;
use crate::scratch::table::SCRATCH_SERIAL;
use crate::EngineError;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[test]
fn namespaces_share_one_connection_and_isolate_keys_and_queues() {
    let anchor = std::env::temp_dir().join(format!(
        "layerfs-scratch-namespaces-{}-{}",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&anchor).unwrap();
    let table = DiskTable::create_near(&anchor, "namespaces").unwrap();
    let expected_pages = assert_default_cache_budget(&table);
    let statements = table.observation().unwrap().statements;
    table.set_cache_size_kib(DISK_TABLE_CACHE_KIB).unwrap();
    assert_eq!(table.observation().unwrap().statements, statements + 3);
    assert_eq!(assert_default_cache_budget(&table), expected_pages);
    let first = table.namespace(b"first").unwrap();
    let second = table.namespace(b"second").unwrap();

    first.put(b"same", b"one").unwrap();
    second.put(b"same", b"two").unwrap();
    first.enqueue_once(b"queue", b"first").unwrap();
    second.enqueue_once(b"queue", b"second").unwrap();
    assert_eq!(first.get(b"same").unwrap(), Some(b"one".to_vec()));
    assert_eq!(second.get(b"same").unwrap(), Some(b"two".to_vec()));
    assert_eq!(
        first.pop_pending().unwrap(),
        Some((b"queue".to_vec(), b"first".to_vec()))
    );
    assert_eq!(
        second.pop_pending().unwrap(),
        Some((b"queue".to_vec(), b"second".to_vec()))
    );
    first
        .for_each_entry(|key, value| {
            assert!(matches!(key, b"queue" | b"same"));
            assert!(matches!(value, b"first" | b"one"));
            assert_eq!(second.get(b"same")?, Some(b"two".to_vec()));
            second.put(b"nested", value)?;
            Ok(())
        })
        .unwrap();
    assert!(second.get(b"nested").unwrap().is_some());
    first.clear().unwrap();
    assert_eq!(first.get(b"same").unwrap(), None);
    assert_eq!(first.pop_pending().unwrap(), None);
    assert_eq!(second.get(b"same").unwrap(), Some(b"two".to_vec()));
    assert!(second.get(b"nested").unwrap().is_some());

    let observation = table.observation().unwrap();
    assert_eq!(observation.tables, 1);
    assert!(observation.rows > 0);
    assert!(observation.high_water_bytes > 0);
    drop(second);
    drop(first);
    drop(table);
    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}
#[test]
fn namespace_ordered_batch_is_bounded_ordered_and_counts_present_rows() {
    let anchor = std::env::temp_dir().join(format!(
        "layerfs-scratch-batch-{}-{}",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&anchor).unwrap();
    let path = {
        let table = DiskTable::create_near(&anchor, "batch").unwrap();
        let path = table.path.clone();
        let first = table.namespace(b"first").unwrap();
        let second = table.namespace(b"second").unwrap();
        first.put(b"a", b"one").unwrap();
        first.put(b"b", b"two").unwrap();
        second.put(b"a", b"other").unwrap();

        let before = table.observation().unwrap();
        first
            .get_ordered_batch(&[], |_, _| panic!("empty batch callback"))
            .unwrap();
        assert_eq!(table.observation().unwrap(), before);

        let mut values = Vec::new();
        first
            .get_ordered_batch(&[b"b", b"a", b"b"], |ordinal, value| {
                values.push((ordinal, value.map(<[u8]>::to_vec)));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            values,
            vec![
                (0, Some(b"two".to_vec())),
                (1, Some(b"one".to_vec())),
                (2, Some(b"two".to_vec())),
            ]
        );
        let after_ordered = table.observation().unwrap();
        assert_eq!(after_ordered.statements, before.statements + 1);
        assert_eq!(after_ordered.rows, before.rows + 3);

        let mut missing = Vec::new();
        first
            .get_ordered_batch(
                &[
                    b"missing-first".as_slice(),
                    b"a".as_slice(),
                    b"missing-middle".as_slice(),
                    b"b".as_slice(),
                    b"missing-last".as_slice(),
                ],
                |ordinal, value| {
                    missing.push((ordinal, value.map(<[u8]>::to_vec)));
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(
            missing,
            vec![
                (0, None),
                (1, Some(b"one".to_vec())),
                (2, None),
                (3, Some(b"two".to_vec())),
                (4, None),
            ]
        );
        let after_missing = table.observation().unwrap();
        assert_eq!(after_missing.statements, after_ordered.statements + 1);
        assert_eq!(after_missing.rows, after_ordered.rows + 2);

        let mut isolated = None;
        second
            .get_ordered_batch(&[b"a"], |_, value| {
                isolated = value.map(<[u8]>::to_vec);
                Ok(())
            })
            .unwrap();
        assert_eq!(isolated, Some(b"other".to_vec()));

        let repeated = vec![b"a".as_slice(); 64];
        let before_64 = table.observation().unwrap();
        let mut seen = 0;
        first
            .get_ordered_batch(&repeated, |ordinal, value| {
                assert_eq!(ordinal, seen);
                assert_eq!(value, Some(b"one".as_slice()));
                seen += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(seen, 64);
        let after_64 = table.observation().unwrap();
        assert_eq!(after_64.statements, before_64.statements + 1);
        assert_eq!(after_64.rows, before_64.rows + 64);

        let oversized = vec![b"a".as_slice(); 65];
        assert!(matches!(
            first.get_ordered_batch(&oversized, |_, _| Ok(())),
            Err(EngineError::InvalidRecord("scratch batch exceeds 64"))
        ));
        assert_eq!(table.observation().unwrap(), after_64);
        assert!(matches!(
            first.get_ordered_batch(&[b"a"], |_, _| {
                Err(EngineError::InjectedFailure("batch callback"))
            }),
            Err(EngineError::InjectedFailure("batch callback"))
        ));

        table
            .connection()
            .execute(
                "UPDATE entries SET value = 'bad' WHERE key = ?1",
                params![first.key(b"a")],
            )
            .unwrap();
        assert!(matches!(
            first.get_ordered_batch(&[b"a"], |_, _| Ok(())),
            Err(EngineError::InvalidRecord("scratch value"))
        ));
        path
    };
    assert!(!path.exists());
    assert!(!PathBuf::from(format!("{}-journal", path.display())).exists());
    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}
