use super::*;
use crate::sqlite::connection::CommitDispatch;
use layerfs_core::namespace::NamespaceRootV1;
use layerfs_core::namespace_codec::{encode_namespace_root, profile_id};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD_TEST: &str = "publication::tests::publication_crash_child";

struct LostCommitAcknowledgement;
impl CommitDispatch for LostCommitAcknowledgement {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("COMMIT")?;
        Err(rusqlite::Error::InvalidQuery)
    }
}

fn root(label: &str) -> Vec<u8> {
    encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x71; 32], 0),
        inode_table_root: ObjectId::for_bytes(label.as_bytes()),
    })
    .unwrap()
}

#[test]
fn publication_crash_child() {
    let Ok(cut) = std::env::var("LAYERFS_PUBLICATION_CRASH_CUT") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("LAYERFS_PUBLICATION_CRASH_STORE").unwrap());
    let mut engine =
        Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    if cut == "T6-requested" {
        engine.commit_dispatch = std::sync::Arc::new(LostCommitAcknowledgement);
    }
    let prior = engine.read_ref("main").unwrap();
    if cut == "T6-different" {
        let replacement_path = path.with_extension("t6-replacement");
        let replacement =
            Engine::open_with_mode(&replacement_path, integrity::IntegrityMode::TrustedLocalDev)
                .unwrap();
        replacement
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&root("replacement"))
            .unwrap();
    }
    if cut == "T0" {
        std::process::exit(80);
    }
    let publication = engine.begin_publication(prior.as_ref(), "main").unwrap();
    if cut == "T1" {
        std::process::exit(81);
    }
    if matches!(cut.as_str(), "T2" | "T3" | "T4") {
        let wanted = match cut.as_str() {
            "T2" => "layerfs_objects",
            "T3" => "layerfs_retained_roots",
            "T4" => "layerfs_refs",
            _ => unreachable!(),
        };
        publication
            .connection
            .update_hook(Some(
                move |_: rusqlite::hooks::Action, _: &str, table: &str, _: i64| {
                    if table == wanted {
                        std::process::exit(82);
                    }
                },
            ))
            .unwrap();
    }
    if cut == "T6-prior" {
        publication.connection.commit_hook(Some(|| true)).unwrap();
        assert!(publication.publish_namespace(&root(&cut)).is_err());
        return;
    }
    if matches!(cut.as_str(), "T6-different" | "T6-missing") {
        let hook_path = path.clone();
        let different = cut == "T6-different";
        publication
            .connection
            .commit_hook(Some(move || {
                fs::rename(&hook_path, hook_path.with_extension("t6-old")).unwrap();
                if different {
                    fs::rename(hook_path.with_extension("t6-replacement"), &hook_path).unwrap();
                }
                true
            }))
            .unwrap();
        assert!(matches!(
            publication.publish_namespace(&root(&cut)),
            Err(EngineError::AmbiguousDurability)
        ));
        assert!(matches!(
            engine.read_ref("main"),
            Err(EngineError::AmbiguousDurability)
        ));
        return;
    }
    publication.publish_namespace(&root(&cut)).unwrap();
}

#[test]
fn publication_t0_through_t6_survive_real_process_exit_and_reopen() {
    for cut in [
        "T0",
        "T1",
        "T2",
        "T3",
        "T4",
        "T5",
        "T6-prior",
        "T6-requested",
        "T6-different",
        "T6-missing",
    ] {
        let path = std::env::temp_dir().join(format!(
            "layerfs-publication-crash-{cut}-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        let prior = engine
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&root("prior"))
            .unwrap();
        let store_id = engine.store_id().unwrap();
        drop(engine);

        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST])
            .env("LAYERFS_PUBLICATION_CRASH_CUT", cut)
            .env("LAYERFS_PUBLICATION_CRASH_STORE", &path)
            .status()
            .unwrap();
        assert_eq!(
            status.success(),
            matches!(
                cut,
                "T5" | "T6-prior" | "T6-requested" | "T6-different" | "T6-missing"
            )
        );

        if cut == "T6-different" {
            let replacement =
                Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
            assert_ne!(replacement.store_id().unwrap(), store_id);
            let state = replacement.read_ref("main").unwrap().unwrap();
            decode_namespace_root(&replacement.load_object(state.root).unwrap().canonical_bytes)
                .unwrap();
            drop(replacement);
            fs::remove_file(&path).unwrap();
            fs::remove_file(path.with_extension("t6-old")).unwrap();
            continue;
        }
        if cut == "T6-missing" {
            assert!(!path.exists());
            fs::rename(path.with_extension("t6-old"), &path).unwrap();
        }

        let reopened =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        let observed = reopened.read_ref("main").unwrap().unwrap();
        decode_namespace_root(&reopened.load_object(observed.root).unwrap().canonical_bytes)
            .unwrap();
        if matches!(cut, "T5" | "T6-requested") {
            assert_ne!(observed.root, prior.root, "{cut} lost requested root");
            assert_eq!(observed.generation, prior.generation + 1);
        } else {
            assert_eq!(observed, prior, "{cut} exposed partial publication");
        }
        drop(reopened);
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn lost_commit_acknowledgement_counts_reconciliation_and_reopen_sql() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-publication-reconciliation-counters-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut engine =
        Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    engine.commit_dispatch = std::sync::Arc::new(LostCommitAcknowledgement);
    engine.reset_counters().unwrap();

    engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root("requested"))
        .unwrap();
    let counters = engine.counters().unwrap();
    assert_eq!(counters.reconciliation_statements, 84);
    assert_eq!(
        counters.statements,
        counters.publication_statements + counters.reconciliation_statements
    );
    assert_eq!(counters.primary_read_statements, 0);
    assert_eq!(counters.live_verified_integrity_statements, 0);
    assert_eq!(counters.compaction_statements, 0);

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn restore_primary_rejects_same_store_id_stale_path_replacement() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-publication-stale-reopen-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let prior = engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root("prior"))
        .unwrap();
    drop(engine);
    let stale = path.with_extension("stale");
    fs::copy(&path, &stale).unwrap();

    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let requested = engine
        .begin_publication(Some(&prior), "main")
        .unwrap()
        .publish_namespace(&root("requested"))
        .unwrap();
    let store_id = engine.store_id().unwrap();
    let saved = path.with_extension("requested");
    fs::rename(&path, &saved).unwrap();
    fs::rename(&stale, &path).unwrap();

    let mut connection = engine.lock_connection().unwrap();
    connection.guard.take();
    assert!(matches!(
        restore_primary(&engine, &mut connection, store_id, "main", &Some(requested)),
        Err(EngineError::AmbiguousDurability)
    ));
    assert!(connection.guard.is_none());
    drop(connection);
    assert!(matches!(
        engine.read_ref("main"),
        Err(EngineError::AmbiguousDurability)
    ));
    drop(engine);
    fs::remove_file(path).unwrap();
    fs::remove_file(saved).unwrap();
}
