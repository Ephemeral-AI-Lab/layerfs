//! Unbounded persisted facts are streamed without altering Commit identity.
mod support;
use layerfs_history::{
    CommitMeta, CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog,
    StackInitialization,
};
use support::doctor::{clean, failure, Fixture};
fn fixture() -> Fixture {
    let f = Fixture::new();
    let stack = f
        .handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack: support::stack(1),
            name: support::name("facts"),
            scope: support::root(1),
            profile: support::root(2),
            genesis_root: support::root(3),
            logical: 0,
        })
        .unwrap();
    let branch = support::branch_id(1);
    f.handles
        .history
        .fork(&ForkRequest {
            branch,
            stack: stack.id,
            name: support::name("main"),
            source: ForkSource::Layer(stack.head_layer),
        })
        .unwrap();
    let snapshot = f.handles.history.branch_snapshot(branch).unwrap().unwrap();
    let mut meta = CommitMeta::new(CommitVia::Files);
    meta.message = Some("facts".to_owned());
    f.handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: None,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            candidate_root: support::root(4),
            profile: snapshot.profile,
            scope: snapshot.scope,
            second_parent: None,
            force: false,
            meta,
        })
        .unwrap();
    f
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}
#[test]
fn streamed_facts_accept_large_split_utf8_and_units_with_exact_eof() {
    let f = fixture();
    clean(&f.audit(true));
    let mut blob = Vec::new();
    put_text(
        &mut blob,
        &format!("{}€{}", "a".repeat(8191), "z".repeat(140_000)),
    );
    put_text(&mut blob, "rule");
    blob.push(3);
    blob.extend_from_slice(&[255; 4]);
    blob.extend_from_slice(&1u32.to_be_bytes());
    put_text(&mut blob, "unit");
    blob.extend_from_slice(&2u32.to_be_bytes());
    put_text(&mut blob, "path/**");
    put_text(&mut blob, "*.data");
    f.raw()
        .execute("UPDATE \"commit\" SET lww=?1", [blob])
        .unwrap();
    let out = f.audit(true);
    clean(&out);
    assert!(out
        .iter()
        .any(|o| o.id == "store.history.facts.summary" && o.message.starts_with("1 persisted")));
}
#[test]
fn malformed_fact_flags_utf8_extents_and_trailing_units_are_not_identity_changes() {
    for blob in [
        vec![0, 0, 0, 8, b'a'],
        vec![0, 0, 0, 1, 255, 0, 0, 0, 0, 0],
        vec![0, 0, 0, 0, 0, 0, 0, 0, 4],
        vec![255, 255, 255, 255, 0, 0, 0, 0],
    ] {
        let f = fixture();
        clean(&f.audit(true));
        f.raw()
            .execute("UPDATE \"commit\" SET lww=?1", [blob])
            .unwrap();
        let out = f.audit(true);
        failure(&out, "store.history.facts");
        assert!(
            !out.iter()
                .any(|o| o.id == "store.history.record" && o.message.contains("identity")),
            "facts are outside identity: {out:?}"
        );
    }
}

#[test]
fn sqlite_nul_prefixed_text_cannot_bypass_blob_or_text_byte_guards() {
    for field in ["message", "name", "root_id", "lww"] {
        let f = fixture();
        clean(&f.audit(true));
        let suffix = "x".repeat(1024 * 1024);
        let raw = f.raw();
        raw.execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        match field {
            "message" => {
                raw.execute("UPDATE \"commit\" SET message=?1", [format!("\0{suffix}")])
                    .unwrap();
            }
            "name" => {
                raw.execute("UPDATE branch SET name=?1", [format!("a\0{suffix}")])
                    .unwrap();
            }
            "root_id" => {
                assert!(raw
                    .execute(
                        "UPDATE \"commit\" SET root_id=?1",
                        [format!("{}\0{suffix}", "a".repeat(32))]
                    )
                    .is_err());
                continue;
            }
            _ => {
                assert!(raw
                    .execute("UPDATE \"commit\" SET lww=?1", [format!("\0{suffix}")])
                    .is_err());
                continue;
            }
        }
        let out = f.audit(true);
        failure(
            &out,
            if field == "lww" {
                "store.history.facts"
            } else {
                "store.history.record"
            },
        );
    }
}
