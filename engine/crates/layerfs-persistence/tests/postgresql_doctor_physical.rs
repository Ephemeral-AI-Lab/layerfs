//! Actual PostgreSQL amcheck applicability, replica and physical payload matrix.
#![cfg(feature = "postgresql")]
mod pg_doctor;
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_persistence::{diagnostic::Outcome, PostgresHandles};
use layerfs_storage::Storage;
use pg_doctor::*;
use std::time::{Duration, Instant};
#[test]
fn supported_amcheck_reports_each_relation_and_missing_extension_is_unknown() {
    let Some(mut f) = Fixture::new("physical") else {
        return;
    };
    let before = f.fingerprint();
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    assert!(
        has(&out, "store.database.relations.summary", Outcome::Pass),
        "{out:?}"
    );
    assert!(
        has(&out, "store.database.relations", Outcome::Pass),
        "successful relations must be identifiable: {out:?}"
    );
    let expected:i64=f.admin.query_one("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','i','m','S')",&[&f.selection.schema]).unwrap().get(0);
    let actual = out
        .iter()
        .filter(|o| o.id == "store.database.relations" && o.outcome == Outcome::Pass)
        .count() as i64;
    assert_eq!(actual, expected);
    drop(reader);
    assert_eq!(before, f.fingerprint());
    let missing = std::env::var("LAYERFS_TEST_POSTGRES_MISSING_URL").unwrap();
    let mut f = Fixture::at("no_extension", missing).unwrap();
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    assert!(
        has(&out, "store.database.relations.summary", Outcome::Unknown),
        "{out:?}"
    );
    let count: i64 = f
        .admin
        .query_one(
            "SELECT count(*) FROM pg_extension WHERE extname='amcheck'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 0, "Doctor did not install extension");
}
#[test]
fn denied_amcheck_and_non_btree_indexes_leave_explicit_incomplete_coverage() {
    let Some(mut f) = Fixture::new("am_denied") else {
        return;
    };
    let role = f.role();
    let restricted = f.restricted(&role);
    let reader = PostgresHandles::open_diagnostic(
        &restricted,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .unwrap();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    assert!(
        has(&out, "store.database.relations.summary", Outcome::Unknown),
        "{out:?}"
    );
    assert!(
        out.iter()
            .any(|o| o.message.contains("EXECUTE privilege is denied")),
        "{out:?}"
    );
    drop(reader);
    f.admin
        .batch_execute(&format!(
            "CREATE INDEX doctor_hash_fixture ON {}.history_meta USING hash (catalog_incarnation)",
            f.selection.schema
        ))
        .unwrap();
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    let oid: i64 = f
        .admin
        .query_one(
            "SELECT ($1::text::regclass)::oid::bigint",
            &[&format!("{}.doctor_hash_fixture", f.selection.schema)],
        )
        .unwrap()
        .get(0);
    assert!(
        out.iter().any(|o| o.id == "store.database.relations"
            && o.outcome == Outcome::Unknown
            && o.message.contains(&format!("OID {oid}: unsupported"))),
        "{out:?}"
    );
}
#[test]
fn physical_standby_uses_supported_online_checks_without_primary_writes() {
    let Some(f) = Fixture::new("replica") else {
        return;
    };
    let url = std::env::var("LAYERFS_TEST_POSTGRES_REPLICA_URL").unwrap();
    let mut monitor = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    assert!(monitor
        .query_one("SELECT pg_is_in_recovery()", &[])
        .unwrap()
        .get::<_, bool>(0));
    let until = Instant::now() + Duration::from_secs(5);
    while !monitor
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_namespace WHERE nspname=$1)",
            &[&f.selection.schema],
        )
        .unwrap()
        .get::<_, bool>(0)
    {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    let selected = layerfs_persistence::PostgresSelection::new(url, &f.selection.schema);
    let reader = PostgresHandles::open_diagnostic(
        &selected,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .unwrap();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    assert!(has(&out, "store.snapshot", Outcome::Pass), "{out:?}");
    assert!(
        has(&out, "store.database.relations.summary", Outcome::Pass),
        "supported online replica checks must work: {out:?}"
    );
    assert_eq!(reader.diagnostics().unwrap().write_transactions, 0);
    drop(reader);
}
#[test]
fn oversized_physical_payloads_are_rejected_before_client_materialization() {
    let Some(mut f) = Fixture::new("oversize") else {
        return;
    };
    let storage = Storage::new(f.writer.as_ref().unwrap().storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"original bounded pack").unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
    drop(storage);
    // Fault setup only: disable the immutable guard and the actual-body constraint,
    // then retain a valid descriptor while creating a much larger EXTERNAL body.
    f.admin.batch_execute(&format!("ALTER TABLE {s}.pack DISABLE TRIGGER pack_immutable; ALTER TABLE {s}.pack DROP CONSTRAINT pack_check; UPDATE {s}.pack SET body=body||decode(repeat('00',1048576),'hex')",s=f.selection.schema)).unwrap();
    use std::str::FromStr;
    let config = postgres::Config::from_str(&f.selection.url).unwrap();
    let port = config.get_ports()[0];
    let proxy = proxy::Proxy::new(port);
    let selected = layerfs_persistence::PostgresSelection::new(
        f.selection
            .url
            .replace(&format!(":{port}/"), &format!(":{}/", proxy.port)),
        &f.selection.schema,
    );
    let reader = PostgresHandles::open_diagnostic(
        &selected,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .unwrap();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    assert!(has(&out, "store.packs", Outcome::Fail), "{out:?}");
    assert!(
        proxy.bytes() < 512 * 1024,
        "one-megabyte corrupted body crossed wire: {} bytes",
        proxy.bytes()
    );
    drop(reader);
    f.admin.batch_execute(&format!("ALTER TABLE {s}.inode_value_block DROP CONSTRAINT inode_value_block_body_check; INSERT INTO {s}.inode_value_block(first_ordinal,last_ordinal,body) VALUES(100,100,decode(repeat('00',5000),'hex'))",s=f.selection.schema)).unwrap();
    let reader = f.reader();
    let (result, out) = audit(&reader, true);
    result.unwrap();
    assert!(has(&out, "store.ordinal_blocks", Outcome::Fail), "{out:?}");
}

#[test]
fn resealed_retained_object_corruption_propagates_on_the_postgres_route() {
    let Some(mut f) = Fixture::new("resealed") else {
        return;
    };
    let storage = Storage::new(f.writer.as_ref().unwrap().storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let text = b"retained postgres canonical identity";
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(text).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
    drop(storage);
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    drop(reader);
    let row = f
        .admin
        .query_one(
            &format!("SELECT pack_id,body FROM {}.pack", f.selection.schema),
            &[],
        )
        .unwrap();
    let id: i64 = row.get(0);
    let mut body: Vec<u8> = row.get(1);
    let at = body.windows(text.len()).position(|b| b == text).unwrap();
    body[at] ^= 1;
    let digest = layerfs_storage::port::ObjectKey::for_bytes(&body);
    f.admin
        .batch_execute(&format!(
            "ALTER TABLE {}.pack DISABLE TRIGGER pack_immutable",
            f.selection.schema
        ))
        .unwrap();
    f.admin
        .execute(
            &format!(
                "UPDATE {}.pack SET body=$1,digest=$2 WHERE pack_id=$3",
                f.selection.schema
            ),
            &[&body, &digest.as_bytes().as_slice(), &id],
        )
        .unwrap();
    f.admin
        .batch_execute(&format!(
            "ALTER TABLE {}.pack ENABLE TRIGGER pack_immutable",
            f.selection.schema
        ))
        .unwrap();
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    assert!(has(&out, "store.objects", Outcome::Fail), "{out:?}");
    assert!(
        has(&out, "store.packs.summary", Outcome::Pass),
        "recomputed outer pack digest passes; canonical identity still fails: {out:?}"
    );
}

#[test]
fn shared_admission_budget_does_not_become_an_implicit_full_audit_limit() {
    let Some(f) = Fixture::new("admission") else {
        return;
    };
    let until = Instant::now() + Duration::from_secs(2);
    let reader = PostgresHandles::open_diagnostic_admitted(
        &f.selection,
        BINDING,
        [71; 32],
        None,
        until,
        &mut resolver,
    )
    .unwrap();
    std::thread::sleep(until.saturating_duration_since(Instant::now()) + Duration::from_millis(20));
    let mut out = Vec::new();
    reader
        .diagnose(
            layerfs_persistence::diagnostic::Options {
                quick: true,
                deadline: None,
                cancelled: None,
            },
            &mut |o| out.push(o),
        )
        .unwrap();
    no_corruption(&out);
    assert!(has(&out, "store.schema.objects.summary", Outcome::Pass));
}

#[test]
fn concurrent_project_removal_preserves_one_postgres_snapshot_and_retained_objects() {
    use layerfs_history::{HistoryCatalog, LayerStackId};
    let Some(f) = Fixture::new("remove") else {
        return;
    };
    let writer = f.writer.as_ref().unwrap();
    pg_doctor::namespace::project(writer, 1);
    pg_doctor::namespace::project(writer, 2);
    let reader = f.reader();
    let before = reader.diagnostics().unwrap();
    let mut out = Vec::new();
    reader
        .diagnose(
            layerfs_persistence::diagnostic::Options {
                quick: false,
                deadline: Some(Instant::now() + Duration::from_secs(20)),
                cancelled: None,
            },
            &mut |o| {
                if o.id == "store.snapshot" {
                    writer
                        .history
                        .remove_layerstack(LayerStackId::from_authority([1; 16]))
                        .unwrap();
                }
                out.push(o)
            },
        )
        .unwrap();
    no_corruption(&out);
    assert!(
        out.iter()
            .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("2 historical")),
        "{out:?}"
    );
    assert!(writer
        .history
        .layer_stack(LayerStackId::from_authority([1; 16]))
        .unwrap()
        .is_none());
    let (result, next) = audit(&reader, false);
    result.unwrap();
    no_corruption(&next);
    assert!(
        next.iter()
            .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("1 historical")),
        "{next:?}"
    );
    assert!(has(&next, "store.objects.summary", Outcome::Pass));
    let after = reader.diagnostics().unwrap();
    assert_eq!(before.write_transactions, after.write_transactions);
    assert_eq!(before.write_commits, after.write_commits);
}

#[test]
fn pinned_catalog_replacement_is_rejected_as_authority_change() {
    let Some(mut f) = Fixture::new("authority_race") else {
        return;
    };
    let reader = f.reader();
    f.admin
        .batch_execute(&format!(
            "UPDATE {}.history_meta SET catalog_incarnation=8 WHERE id=1",
            f.selection.schema
        ))
        .unwrap();
    let (result, out) = audit(&reader, false);
    assert!(result.is_err());
    assert!(has(&out, "store.identity", Outcome::Fail), "{out:?}");
    assert!(!out.iter().any(|o| o.id == "store.packs.summary"));
}

#[test]
fn oversized_authority_and_saved_ddl_stay_on_server_during_admission() {
    use std::str::FromStr;
    for (tag,mutation) in [("binding","ALTER TABLE {s}.history_meta DROP CONSTRAINT history_meta_binding_key_check; UPDATE {s}.history_meta SET binding_key=decode(repeat('00',1048576),'hex')"),("source","UPDATE {s}.history_meta SET schema_source=repeat('x',1048576)"),("definition","UPDATE {s}.history_meta SET schema_definition=repeat('x',1048576)")]{
 let Some(mut f)=Fixture::new(tag)else{return};f.admin.batch_execute(&mutation.replace("{s}",&f.selection.schema)).unwrap();let before=f.fingerprint();let config=postgres::Config::from_str(&f.selection.url).unwrap();let port=config.get_ports()[0];let proxy=proxy::Proxy::new(port);let selection=layerfs_persistence::PostgresSelection::new(f.selection.url.replace(&format!(":{port}/"),&format!(":{}/",proxy.port)),&f.selection.schema);
 if tag=="binding"{let out=connection(&selection,BINDING);assert!(has(&out,"store.connection.identity",Outcome::Fail),"{out:?}");}else{assert!(PostgresHandles::open_diagnostic(&selection,BINDING,[71;32],Some(Instant::now()+Duration::from_secs(5)),&mut resolver).is_err());}
 assert!(proxy.bytes()<512*1024,"oversized metadata crossed wire: {} bytes",proxy.bytes());drop(proxy);assert_eq!(before,f.fingerprint());
 }
}

#[test]
fn oversized_pack_descriptor_digest_is_rejected_before_wire_extraction() {
    use std::str::FromStr;
    let Some(mut f) = Fixture::new("descriptor") else {
        return;
    };
    let storage = Storage::new(f.writer.as_ref().unwrap().storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"valid descriptor negative control").unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
    drop(storage);
    let reader = f.reader();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    no_corruption(&out);
    drop(reader);
    f.admin.batch_execute(&format!("ALTER TABLE {s}.pack DISABLE TRIGGER pack_immutable; ALTER TABLE {s}.pack DROP CONSTRAINT pack_digest_check; UPDATE {s}.pack SET digest=decode(repeat('00',1048576),'hex'); ALTER TABLE {s}.pack ENABLE TRIGGER pack_immutable",s=f.selection.schema)).unwrap();
    let port = postgres::Config::from_str(&f.selection.url)
        .unwrap()
        .get_ports()[0];
    let proxy = proxy::Proxy::new(port);
    let selected = layerfs_persistence::PostgresSelection::new(
        f.selection
            .url
            .replace(&format!(":{port}/"), &format!(":{}/", proxy.port)),
        &f.selection.schema,
    );
    let reader = PostgresHandles::open_diagnostic(
        &selected,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .unwrap();
    let (result, out) = audit(&reader, false);
    result.unwrap();
    assert!(has(&out, "store.packs", Outcome::Fail), "{out:?}");
    assert!(
        has(&out, "store.objects", Outcome::Fail),
        "locator must independently reject invalid pack descriptor: {out:?}"
    );
    assert!(
        proxy.bytes() < 512 * 1024,
        "oversized digest crossed wire: {} bytes",
        proxy.bytes()
    );
}
