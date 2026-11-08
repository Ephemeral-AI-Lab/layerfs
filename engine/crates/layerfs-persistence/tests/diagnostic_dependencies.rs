//! C03/C04: Doctor inventories real persisted physical dependency graphs.
mod support;
use layerfs_content::{
    filesystem::{
        directory::{encode_directory_page, DirectoryPage},
        PathName,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{pack::layout::PackLane, StoragePolicy};
use support::doctor::{clean, failure, Fixture};
fn object(name: &str) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: vec![(PathName::new(name).unwrap(), 7)],
        })
        .unwrap(),
    )
    .unwrap()
}
fn delta(base: ObjectId, body: &[u8]) -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(base.as_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.push(1);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}
fn full(body: &[u8]) -> Vec<u8> {
    let mut out = vec![0];
    out.extend_from_slice(body);
    out
}
#[test]
fn c04_valid_forward_dependencies_and_missing_bases_are_distinguished() {
    for missing in [false, true] {
        let f = Fixture::new();
        let a = object("a");
        let b = object("b");
        f.record(&a, PackLane::Ordinary, delta(b.id(), a.canonical()), vec![]);
        if !missing {
            f.record(&b, PackLane::Ordinary, full(b.canonical()), vec![]);
        }
        let out = f.audit(false);
        if missing {
            failure(&out, "store.objects");
            assert!(out.iter().any(|o| o.message.contains(&a.id().to_string())));
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c04_actual_cycle_is_rejected_without_requiring_physical_chronology() {
    for cycle in [false, true] {
        let f = Fixture::new();
        let a = object("a");
        let b = object("b");
        f.record(&a, PackLane::Ordinary, delta(b.id(), a.canonical()), vec![]);
        f.record(
            &b,
            PackLane::Ordinary,
            if cycle {
                delta(a.id(), b.canonical())
            } else {
                full(b.canonical())
            },
            vec![],
        );
        let out = f.audit(false);
        if cycle {
            failure(&out, "store.objects");
            assert!(out.iter().any(|o| o.message.contains("cycle")), "{out:?}");
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c04_fully_replacing_delta_still_authenticates_required_base() {
    for corrupt in [false, true] {
        let f = Fixture::new();
        let a = object("a");
        let b = object("b");
        f.record(
            &a,
            PackLane::Ordinary,
            full(if corrupt {
                b.canonical()
            } else {
                a.canonical()
            }),
            vec![],
        );
        f.record(&b, PackLane::Ordinary, delta(a.id(), b.canonical()), vec![]);
        let out = f.audit(false);
        if corrupt {
            failure(&out, "store.objects");
            assert!(
                out.iter().any(
                    |o| o.outcome == layerfs_persistence::diagnostic::Outcome::Fail
                        && o.message.contains(&b.id().to_string())
                ),
                "dependent must fail too: {out:?}"
            );
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c04_metadata_depth_boundary_and_wrong_base_role_are_checked() {
    for depth in [1u8, 2] {
        let f = Fixture::with_policy(StoragePolicy::frozen_default().with_metadata_depth(depth));
        let a = object("a");
        let b = object("b");
        let c = object("c");
        f.record(&a, PackLane::Ordinary, full(a.canonical()), vec![]);
        f.record(&b, PackLane::Ordinary, delta(a.id(), b.canonical()), vec![]);
        f.record(&c, PackLane::Ordinary, delta(b.id(), c.canonical()), vec![]);
        let out = f.audit(false);
        if depth == 1 {
            failure(&out, "store.objects");
        } else {
            clean(&out);
        }
    }
    let f = Fixture::new();
    let a = object("a");
    let b = object("b");
    f.record(&a, PackLane::Ordinary, full(a.canonical()), vec![]);
    f.record(&b, PackLane::Ordinary, delta(a.id(), b.canonical()), vec![]);
    clean(&f.audit(false));
    f.raw()
        .execute(
            "UPDATE object_location SET role=8 WHERE object_id=?1",
            [a.id().as_bytes().as_slice()],
        )
        .unwrap();
    failure(&f.audit(false), "store.objects");
}
#[test]
fn c03_bad_locators_do_not_hide_independent_valid_objects() {
    for sql in [
        "UPDATE object_location SET role=8 WHERE object_id=?1",
        "UPDATE object_location SET canonical_length=canonical_length+1 WHERE object_id=?1",
        "UPDATE object_location SET group_number=1 WHERE object_id=?1",
        "UPDATE object_location SET record_number=1 WHERE object_id=?1",
    ] {
        let f = Fixture::new();
        let a = object("a");
        let b = object("b");
        f.record(&a, PackLane::Ordinary, full(a.canonical()), vec![]);
        f.record(&b, PackLane::Ordinary, full(b.canonical()), vec![]);
        clean(&f.audit(false));
        f.raw()
            .execute(sql, [a.id().as_bytes().as_slice()])
            .unwrap();
        let out = f.audit(false);
        failure(&out, "store.objects");
        assert!(
            out.iter()
                .any(|o| o.id == "store.objects.summary" && o.message.starts_with("2 winning")),
            "{out:?}"
        );
    }
}
#[test]
fn c03_missing_winning_location_is_not_supplied_by_a_losing_physical_copy() {
    let f = Fixture::new();
    let a = object("a");
    let b = object("b");
    f.record(&a, PackLane::Ordinary, full(a.canonical()), vec![]);
    f.record(&b, PackLane::Ordinary, delta(a.id(), b.canonical()), vec![]);
    clean(&f.audit(false));
    let raw = f.raw();
    raw.execute(
        "DELETE FROM object_location WHERE object_id=?1",
        [a.id().as_bytes().as_slice()],
    )
    .unwrap();
    raw.execute(
        "UPDATE store_totals SET canonical=canonical-?1",
        [a.canonical_len() as i64],
    )
    .unwrap();
    let out = f.audit(false);
    failure(&out, "store.objects");
    assert!(out
        .iter()
        .any(|o| o.id == "store.objects" && o.message.contains(&b.id().to_string())));
    assert!(out
        .iter()
        .any(|o| o.id == "store.packs.summary" && o.message.starts_with("2 packs")));
}
