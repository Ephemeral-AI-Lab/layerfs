//! #758 phase 2: `pack.directory`, each pack's header and group directory
//! (`body[0..body_offset)`), written by publication with the body, refused by
//! the schema when it is not that prefix, and returned by the chain locate
//! for the roots' packs and the members' packs.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/pg_chains.rs"]
mod chains;

use chains::*;
use layerfs_storage::{
    location::LocatedChains,
    pack::layout,
    policy::DEPENDENCY_PACK_CACHE_BYTES,
    port::{GroupRead, GroupReadAnswer, ObjectKey, PackPersistence, PackRange, PersistenceError},
    Storage,
};
use std::collections::BTreeSet;

#[test]
fn every_directory_is_its_body_prefix() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "dir_prefix");
    let storage = storage(&store.handles);
    let mut files = Files::new(5, 30_000, 11);
    for version in 0..6 {
        files.step(&storage, version % 3 == 2);
    }
    let packs = store
        .admin()
        .query(
            "SELECT pack_id,body,directory FROM pack ORDER BY pack_id",
            &[],
        )
        .unwrap();
    assert!(packs.len() > 5);
    for pack in &packs {
        let (id, body, directory): (i64, Vec<u8>, Vec<u8>) =
            (pack.get(0), pack.get(1), pack.get(2));
        let header = layout::parse_header(&body).unwrap();
        assert_eq!(directory, body[..header.body_offset], "pack {id}");
    }
}

#[test]
fn the_schema_refuses_a_directory_that_is_not_the_body_prefix() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "dir_check");
    save(&storage(&store.handles), vec![whole(&noise(5, 9_000))]);
    let mut admin = store.admin();
    let copy = |admin: &mut postgres::Client, directory: &str| {
        admin.execute(
            &format!(
                "INSERT INTO pack(pack_id,domain,digest,length,body,directory) \
                 SELECT pack_id+1000000,domain,digest,length,body,{directory} FROM pack ORDER BY pack_id LIMIT 1"
            ),
            &[],
        )
    };
    for (bad, what) in [
        ("set_byte(directory,octet_length(directory)-1,(get_byte(directory,octet_length(directory)-1)+1)%256)", "a changed byte"),
        ("directory||'\\x00'::bytea", "one byte too long"),
        ("substring(body FROM 1 FOR 23)", "shorter than the header"),
    ] {
        let error = copy(&mut admin, bad).expect_err(what);
        assert_eq!(
            error.code().map(|c| c.code()),
            Some("23514"),
            "{what}: {error}"
        );
    }
    // the exact prefix is accepted
    copy(&mut admin, "directory").unwrap();
}

#[test]
fn the_chain_locate_returns_the_directories_of_root_and_member_packs() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "dir_chain");
    let storage = storage(&store.handles);
    let mut files = Files::new(3, 24_000, 13);
    for _ in 0..7 {
        files.step(&storage, false);
    }
    let roots = files.latest();
    let handles = store.open();
    let (mut rows, mut chains) = (Vec::new(), LocatedChains::default());
    handles
        .storage
        .locate_chains(&roots, &mut rows, &mut chains)
        .unwrap();
    assert_eq!(rows.len(), roots.len());
    assert_eq!(chains.chains.len(), roots.len(), "every root is a delta");
    let mut expected = BTreeSet::new();
    for row in &rows {
        expected.insert(row.pack.pack_id);
    }
    for chain in &chains.chains {
        let stored = row(&store, chain.root);
        let entries = decode(
            stored.location.pack_id,
            stored.ancestors.as_deref().unwrap(),
        );
        assert_eq!(chain.members.len(), entries.len());
        for (member, entry) in chain.members.iter().zip(&entries) {
            assert_eq!(member.object_id, None, "a physical list carries no ids");
            assert_eq!(
                (
                    member.pack.pack_id,
                    member.group_number,
                    member.record_number,
                    member.canonical_length
                ),
                *entry
            );
            expected.insert(member.pack.pack_id);
        }
    }
    let got: BTreeSet<i64> = chains.directories.iter().map(|d| d.pack.pack_id).collect();
    assert_eq!(got, expected, "one directory per root and member pack");
    assert_eq!(chains.directories.len(), got.len(), "each once");
    let mut admin = store.admin();
    for directory in &chains.directories {
        let body: Vec<u8> = admin
            .query_one(
                "SELECT body FROM pack WHERE pack_id=$1",
                &[&directory.pack.pack_id],
            )
            .unwrap()
            .get(0);
        let header = layout::parse_header(&body).unwrap();
        assert_eq!(directory.prefix, body[..header.body_offset]);
        assert_eq!(directory.pack.length, body.len());
    }
}

#[test]
fn classified_group_reads_bind_missing_width_and_digest_to_each_pack() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "classified_groups");
    let storage = storage(&store.handles);
    let object = whole(&noise(7, 9_000));
    let id = object.id();
    save(&storage, vec![object]);
    let handles = store.open();
    let mut rows = Vec::new();
    handles.storage.locate(&[id], &mut rows).unwrap();
    let pack = rows[0].pack;
    let good = GroupRead {
        pack,
        ranges: Vec::new(),
    };
    let mut missing = good.clone();
    missing.pack.pack_id += 1_000_000;
    let mut short = good.clone();
    short.ranges.push(PackRange {
        offset: pack.length - 1,
        length: 2,
    });
    let mut bad_digest = good.clone();
    let mut wrong_key = *pack.key.as_bytes();
    wrong_key[0] ^= 1;
    bad_digest.pack.key = ObjectKey::from_bytes(wrong_key);
    let mut bad_width = good.clone();
    bad_width.pack.length -= 1;
    let mut ranged = good.clone();
    ranged.ranges.push(PackRange {
        offset: 1,
        length: 5,
    });

    let mut out = Vec::new();
    handles
        .storage
        .read_groups_classified(&[good.clone()], &mut out)
        .unwrap();
    let GroupReadAnswer::Whole(expected) = out.pop().unwrap().unwrap() else {
        panic!("whole pack expected");
    };
    let before = handles.diagnostics().unwrap();
    handles
        .storage
        .read_groups_classified(
            &[missing, short, good.clone(), bad_digest, ranged, bad_width],
            &mut out,
        )
        .unwrap();
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.statements - before.statements, 1);
    assert_eq!(after.round_trips - before.round_trips, 1);
    assert_eq!(after.prepares - before.prepares, 0);
    let bytes = expected.body()[1..6].to_vec();
    assert_eq!(
        out,
        [
            Err(PersistenceError::Missing),
            Err(PersistenceError::Malformed),
            Ok(GroupReadAnswer::Whole(expected)),
            Err(PersistenceError::Malformed),
            Ok(GroupReadAnswer::Ranges(vec![bytes])),
            Err(PersistenceError::Malformed),
        ],
    );

    let mut oversize = good;
    oversize.pack.length = DEPENDENCY_PACK_CACHE_BYTES + 1;
    assert_eq!(
        handles
            .storage
            .read_groups_classified(&[oversize], &mut out),
        Err(PersistenceError::Malformed),
    );
    assert_eq!(
        handles.diagnostics().unwrap().round_trips,
        after.round_trips
    );
    assert!(out.is_empty());
}

/// One demand whose needed groups pass 2 MiB (#758 phase 3): one locate
/// wave, then one ranged group read per page of at most 2 MiB of returned
/// bytes, each consumed before the next. The amended A8 holds: at most 3
/// round trips per wave with chains plus 1 per 2 MiB returned.
#[test]
fn a_demand_past_2_mib_reads_its_groups_in_ranged_pages() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "pages");
    let storage = storage(&store.handles);
    let mut files = Files::new(48, 60_000, 13);
    for _ in 0..6 {
        files.step(&storage, false);
    }
    let heads = files.latest();
    let handles = store.open();
    let (plain, _) = cold_read(&handles, &heads, false);
    let reader = Storage::new(handles.storage.clone()).unwrap();
    reader.set_strict_hints(true);
    let before = handles.diagnostics().unwrap();
    let read = reader.reader().unwrap().read_objects(&heads);
    let after = handles.diagnostics().unwrap();
    let work = reader.diagnostics();
    let cost = Cost::of(&before, &after, &work);
    eprintln!("{cost:?}");
    assert_eq!(read.unwrap(), plain.unwrap());
    assert_eq!(
        (cost.waves, cost.stale, cost.prepares),
        (1, 0, 0),
        "{cost:?}"
    );
    assert!(cost.read_bytes > 2 << 20, "{cost:?}");
    assert!(cost.group_reads >= 2, "{cost:?}");
    assert!(work.group_read_ranges > 0, "{work:?}");
    assert_eq!(work.read_pack_selections + work.read_packs, 0, "{work:?}");
    assert_eq!(cost.round_trips, cost.statements, "{cost:?}");
    assert!(cost.round_trips <= cost.a8_budget(), "{cost:?}");
}
