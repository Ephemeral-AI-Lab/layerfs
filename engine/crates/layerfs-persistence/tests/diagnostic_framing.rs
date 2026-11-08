//! C01 and losing physical representations: full pack checks are independent.
mod support;
use layerfs_content::{
    filesystem::directory::{encode_directory_page, DirectoryPage},
    ObjectRole,
};
use layerfs_storage::{
    location::PackDomain,
    pack::{
        assemble, build_group,
        layout::{self, PackLane},
    },
    port::{PackPersistence, Publication, PublishedPack, Reserve},
};
use std::sync::Arc;
use support::doctor::{clean, failure, Fixture};
fn add_losing(f: &Fixture, body: Vec<u8>) {
    let valid = bytes();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Metadata,
                Arc::new(valid.clone()),
            )],
            ..Publication::default()
        })
        .unwrap();
    if valid != body {
        let raw = f.raw();
        let trigger: String = raw
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='pack_immutable_update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        raw.execute_batch("DROP TRIGGER pack_immutable_update")
            .unwrap();
        let digest = layerfs_storage::port::ObjectKey::for_bytes(&body);
        raw.execute(
            "UPDATE pack SET body=?1,digest=?2,length=?3 WHERE pack_id=?4",
            rusqlite::params![
                body,
                digest.as_bytes().as_slice(),
                body.len() as i64,
                reserved.first_pack_id
            ],
        )
        .unwrap();
        let delta = body.len() as i64 - valid.len() as i64;
        raw.execute(
            "UPDATE store_totals SET stored=stored+?1,metadata=metadata+?1",
            [delta],
        )
        .unwrap();
        raw.execute_batch(&trigger).unwrap();
    }
}
fn bytes() -> Vec<u8> {
    let canonical = encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap();
    let mut record = vec![0];
    record.extend_from_slice(&canonical);
    assemble(
        PackLane::Ordinary,
        &[build_group(PackLane::Ordinary, &[record], None).unwrap()],
    )
    .unwrap()
}
#[test]
fn c01_truncated_and_bad_directory_packs_are_detected_after_resealing() {
    for mutation in 0..3 {
        let f = Fixture::new();
        let mut body = bytes();
        match mutation {
            1 => {
                body.pop();
            }
            2 => {
                body[12] ^= 1;
            }
            _ => {}
        }
        add_losing(&f, body);
        let out = f.audit(false);
        if mutation == 0 {
            clean(&out);
        } else {
            failure(&out, "store.packs");
        }
    }
}
#[test]
fn c01_bad_record_tag_in_unlocated_representation_is_not_hidden_by_winner_inventory() {
    let f = Fixture::new();
    let valid = bytes();
    add_losing(&f, valid.clone());
    clean(&f.audit(false));
    let header = layout::parse_header(&valid).unwrap();
    let view = layout::group_view(&valid, header, 0).unwrap();
    assert_eq!(view.codec, layout::GroupCodec::Raw);
    let mut body = valid;
    body[view.start + 8] = 255;
    add_losing(&f, body);
    let out = f.audit(false);
    failure(&out, "store.packs");
    assert!(
        out.iter().any(|o| o.message.contains("record tag")),
        "{out:?}"
    );
}
#[test]
fn c01_wrong_descriptor_digest_and_length_are_attributable() {
    for sql in [
        "UPDATE pack SET digest=zeroblob(32)",
        "PRAGMA ignore_check_constraints=ON; UPDATE pack SET length=length+1",
    ] {
        let f = Fixture::new();
        f.put(
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap(),
        );
        clean(&f.audit(false));
        let raw = f.raw();
        let trigger: String = raw
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='pack_immutable_update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        raw.execute_batch("DROP TRIGGER pack_immutable_update")
            .unwrap();
        raw.execute_batch(sql).unwrap();
        raw.execute_batch(&trigger).unwrap();
        failure(&f.audit(false), "store.packs");
    }
}
