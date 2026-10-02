//! Disposable real-provider corruption and lost-ACK custody witnesses.
mod strict_witness;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{
    cas::SaveProfile,
    encoding::{encode_prefix, raw_payload, CompressionWorkspace},
    pack::{assemble, build_group},
    StorageCapacities, StoragePolicy,
};
use phase6_live_probe::{
    minio::{digest, hex},
    strict_catalog::{LogicalUse, PlacementDomain, StrictCatalog},
    strict_read::{Access, ReadOwner},
    strict_writer::Writer,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
};
use strict_witness::{disposable, evidence, old_fixture, old_objects, provider};
fn capacities() -> StorageCapacities {
    StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap()
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_digest_valid_altered_intermediate_refuses_canonical_identity() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("refusal-intermediate");
    let catalog = StrictCatalog::create(&path).unwrap();
    let objects = old_objects();
    let bases = [
        ("whole1", "whole0"),
        ("chunk1", "chunk0"),
        ("chunk2", "chunk1"),
        ("pool1", "pool0"),
        ("pool2", "pool1"),
        ("pool3", "pool2"),
    ]
    .into_iter()
    .map(|(child, base)| (objects[child], objects[base]))
    .collect();
    old_fixture(
        &catalog,
        &provider,
        &evidence().join("old-producer-v1/producer.sqlite"),
        None,
        &bases,
    );
    let scope = catalog.capture(None).unwrap();
    let intermediate = catalog
        .location(scope, PlacementDomain::FilePayload, objects["chunk1"])
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            intermediate.location.group_number,
            intermediate.location.record_number
        ),
        (0, 0)
    );
    let canonical = std::fs::read(evidence().join("old-producer-v1/chunk1.canonical")).unwrap();
    let base = std::fs::read(evidence().join("old-producer-v1/chunk0.canonical")).unwrap();
    let mut altered = raw_payload(&canonical, ObjectRole::Chunk).unwrap().to_vec();
    altered[512] ^= 0xff;
    let altered_canonical = layerfs_content::file::mapping::encode_chunk_object(&altered).unwrap();
    let mut encoder = CompressionWorkspace::new().unwrap();
    let record = encode_prefix(
        &altered_canonical,
        ObjectRole::Chunk,
        objects["chunk0"],
        raw_payload(&base, ObjectRole::Chunk).unwrap(),
        &capacities(),
        &mut encoder,
        &mut SaveProfile::default(),
    )
    .unwrap();
    let group = build_group(record.lane, &[record.record], Some(&mut encoder)).unwrap();
    let pack = assemble(record.lane, &[group]).unwrap();
    let key = digest(&pack);
    provider.put(&key, &pack).unwrap();
    assert_eq!(
        provider.get(&key).unwrap(),
        pack,
        "mutated pack SHA authenticates before the dependent read"
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE bodies SET digest=?2 WHERE body_order=?1",
            rusqlite::params![intermediate.location.pack_id, key.as_slice()]
        )
        .unwrap(),
        1
    );
    drop(db);
    let reopened = StrictCatalog::open_read_only(&path).unwrap();
    let access = Access {
        catalog: &reopened,
        provider: &provider,
        scope: reopened.capture(None).unwrap(),
        logical_use: LogicalUse::RegularFileGraph,
    };
    let error = ReadOwner::new()
        .unwrap()
        .read(&access, &capacities(), &[objects["chunk2"]])
        .unwrap_err();
    assert!(
        error.contains("dependency identity"),
        "must authenticate intermediate canonical ID, not merely pack/frame: {error}"
    );
    println!("sp1-refusals-custody-strict-v2 SHA-valid frame-valid altered intermediate refused: {error}");
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_missing_required_base_never_probes_sql_shadow() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("refusal-base-missing");
    let catalog = StrictCatalog::create(&path).unwrap();
    let objects = old_objects();
    let bases = [
        ("whole1", "whole0"),
        ("chunk1", "chunk0"),
        ("chunk2", "chunk1"),
        ("pool1", "pool0"),
        ("pool2", "pool1"),
        ("pool3", "pool2"),
    ]
    .into_iter()
    .map(|(child, base)| (objects[child], objects[base]))
    .collect();
    old_fixture(
        &catalog,
        &provider,
        &evidence().join("old-producer-v1/producer.sqlite"),
        None,
        &bases,
    );
    let scope = catalog.capture(None).unwrap();
    let base = catalog
        .location(scope, PlacementDomain::FilePayload, objects["chunk0"])
        .unwrap()
        .unwrap();
    // External fault setup uses curl's signed DELETE. Minio's product GET/PUT
    // parser requires Content-Length; real MinIO's DELETE204 omits that field.
    // Credentials travel over stdin configuration and are never command args.
    let mut command = std::process::Command::new("curl")
        .args([
            "--config",
            "-",
            "--aws-sigv4",
            "aws:amz:us-east-1:s3",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--silent",
            "--show-error",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let config = format!(
        "request = \"DELETE\"\nuser = \"{}:{}\"\nurl = \"http://{}/{}/{}\"\n",
        provider.access,
        provider.secret,
        provider.authority,
        provider.bucket,
        hex(&base.digest)
    );
    command
        .stdin
        .take()
        .unwrap()
        .write_all(config.as_bytes())
        .unwrap();
    let output = command.wait_with_output().unwrap();
    assert!(output.status.success(), "external signed DELETE failed");
    assert_eq!(std::str::from_utf8(&output.stdout).unwrap(), "204");
    let reopened = StrictCatalog::open_read_only(&path).unwrap();
    let access = Access {
        catalog: &reopened,
        provider: &provider,
        scope: reopened.capture(None).unwrap(),
        logical_use: LogicalUse::RegularFileGraph,
    };
    let before = provider.statistics().unwrap();
    let error = ReadOwner::new()
        .unwrap()
        .read(&access, &capacities(), &[objects["chunk2"]])
        .unwrap_err();
    let after = provider.statistics().unwrap();
    assert_eq!(
        after.put_calls, before.put_calls,
        "read failure cannot resend/adopt"
    );
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let shadows: i64 = db
        .query_row(
            "SELECT count(*) FROM bodies WHERE domain=0 AND metadata IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(shadows, 0);
    assert!(
        error.contains("strict catalog/provider access"),
        "missing provider base refusal: {error}"
    );
    println!("sp1-refusals-custody-strict-v2 missing provider base refused; GETdelta{}, no SQL payload shadow: {error}",after.get_calls-before.get_calls);
}
/// One real forwarder: accepts exactly one PUT, waits for actual MinIO 200, then
/// closes the downstream socket without forwarding that ACK. No product hook.
fn lost_ack_proxy(target: String) -> (String, thread::JoinHandle<(usize, usize, u16)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let handle = thread::spawn(move || {
        let (client, _) = listener.accept().unwrap();
        let mut input = BufReader::new(client);
        let mut header = Vec::new();
        let mut length = None;
        loop {
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            assert!(!line.is_empty());
            header.extend_from_slice(line.as_bytes());
            if let Some((key, value)) = line.split_once(':') {
                if key.eq_ignore_ascii_case("content-length") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            if line == "\r\n" {
                break;
            }
        }
        assert!(header.starts_with(b"PUT "));
        let length = length.unwrap();
        let mut body = vec![0; length];
        input.read_exact(&mut body).unwrap();
        let mut real = TcpStream::connect(target).unwrap();
        real.write_all(&header).unwrap();
        real.write_all(&body).unwrap();
        let mut response = Vec::new();
        real.read_to_end(&mut response).unwrap();
        let status =
            std::str::from_utf8(&response[..response.iter().position(|b| *b == b'\n').unwrap()])
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u16>()
                .unwrap();
        assert_eq!(status, 200);
        drop(input);
        (1, length, status)
    });
    (authority, handle)
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_put_succeeds_lost_ack_quarantines_without_resend_or_delete() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("refusal-lost-ack");
    let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
    let (authority, proxy) = lost_ack_proxy(provider.authority.clone());
    let mut routed = provider.clone();
    routed.authority = authority;
    let mut writer = Writer::new(catalog.clone(), routed.clone(), capacities()).unwrap();
    let canonical = std::fs::read(evidence().join("old-producer-v1/whole0.canonical")).unwrap();
    writer
        .offer(
            LogicalUse::RegularFileGraph,
            FinalizedObject::new(ObjectRole::WholeFile, canonical).unwrap(),
        )
        .unwrap();
    let before = routed.statistics().unwrap().put_calls;
    let error = writer.finish_storage().unwrap_err();
    let (accepted, bytes, status) = proxy.join().unwrap();
    assert_eq!((accepted, status), (1, 200));
    let after = routed.statistics().unwrap().put_calls;
    assert_eq!(after - before, 1);
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let state: i64 = db
        .query_row(
            "SELECT status FROM saves WHERE save_id=?1",
            [writer.save],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, 3, "Unknown custody stays quarantined");
    let (key, ready): (Vec<u8>, i64) = db
        .query_row(
            "SELECT digest,ready FROM bodies WHERE save_id=?1 AND domain=0",
            [writer.save],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ready, 0);
    let key: [u8; 32] = key.try_into().unwrap();
    let retained = provider.get(&key).unwrap();
    assert_eq!(retained.len(), bytes);
    assert_eq!(
        retained,
        std::fs::read(evidence().join("old-producer-v1/pack-1.row")).unwrap()
    );
    assert!(writer.finish_storage().is_err());
    assert!(catalog.publish(writer.save).is_err());
    assert_eq!(routed.statistics().unwrap().put_calls, after);
    println!("sp1-refusals-custody-strict-v2 actual MinIO PUT200 lost ACK Unknown retained {bytes} bytes, one PUT, no resend/delete/publication: {error}");
}

fn noise(length: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..length)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}
fn predecessor(
    object: FinalizedObject,
    previous: Option<layerfs_content::ObjectId>,
) -> FinalizedObject {
    let mut hints = layerfs_content::AdvisoryPredecessors::new();
    if let Some(id) = previous {
        hints
            .push(id, layerfs_content::PredecessorProvenance::OriginalBase)
            .unwrap();
    }
    object.with_predecessors(hints)
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_sealed_depth_and_fixed_chain_work_boundaries() {
    let provider = provider();
    provider.create_bucket().unwrap();
    for chunked in [true, false] {
        let label = if chunked {
            "refusal-depth-boundary"
        } else {
            "refusal-work-boundary"
        };
        let path = disposable(label);
        let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
        let policy = if chunked {
            StoragePolicy::new(1, 131072, 8, 2)
        } else {
            StoragePolicy::new(1, 131072, 16, 4)
        };
        let capacity = StorageCapacities::from_policy(policy).unwrap();
        let length = if chunked { 20000 } else { 120000 };
        let steps = if chunked { 4 } else { 12 };
        let mut raw = noise(length);
        let mut previous = None;
        let mut retained = Vec::new();
        let vector = std::fs::read_to_string(evidence().join(if chunked {
            "strict-oracle-v2/vectors/chunk-depth2.txt"
        } else {
            "strict-oracle-v2/vectors/work-boundary.txt"
        }))
        .unwrap();
        let expected_ids = vector
            .lines()
            .map(|line| strict_witness::id(line.split('\t').nth(1).unwrap()))
            .collect::<Vec<_>>();
        for step in 0..steps {
            let canonical = if chunked {
                layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap()
            } else {
                layerfs_content::encode_whole_file_payload(&raw).unwrap()
            };
            let role = if chunked {
                ObjectRole::Chunk
            } else {
                ObjectRole::WholeFile
            };
            let object = predecessor(
                FinalizedObject::new(role, canonical.clone()).unwrap(),
                previous,
            );
            assert_eq!(object.id(), expected_ids[step]);
            previous = Some(object.id());
            let mut writer = Writer::new(catalog.clone(), provider.clone(), capacity).unwrap();
            writer.offer(LogicalUse::RegularFileGraph, object).unwrap();
            writer.finish_storage().unwrap();
            catalog.publish(writer.save).unwrap();
            let prefix = if chunked {
                matches!(step, 1 | 2)
            } else {
                !matches!(step, 0 | 4 | 8)
            };
            assert_eq!(writer.delta.prefix_selected, u64::from(prefix));
            assert_eq!(writer.delta.trials, u64::from(prefix));
            if chunked && step == 3 {
                assert_eq!(writer.delta.ineligible_candidates, 1);
            }
            if !chunked {
                assert_eq!(writer.delta.work_exceeded, u64::from(matches!(step, 4 | 8)));
            }
            retained.push((previous.unwrap(), canonical));
            println!("sealed {label} step{step}: {:?}", writer.delta);
            if chunked {
                if step == 2 {
                    for byte in &mut raw {
                        *byte ^= 0xa5;
                    }
                } else {
                    for byte in &mut raw[64 + step * 64..96 + step * 64] {
                        *byte ^= 255;
                    }
                }
            } else {
                raw[step * 64..step * 64 + 32].fill(0xa5);
            }
        }
        let catalog = StrictCatalog::open_read_only(&path).unwrap();
        let access = Access {
            catalog: &catalog,
            provider: &provider,
            scope: catalog.capture(None).unwrap(),
            logical_use: LogicalUse::RegularFileGraph,
        };
        let mut reader = ReadOwner::new().unwrap();
        for (object, canonical) in &retained {
            assert_eq!(
                reader.read(&access, &capacity, &[*object]).unwrap(),
                vec![canonical.clone()]
            );
        }
        if chunked {
            let reduced =
                StorageCapacities::from_policy(StoragePolicy::new(1, 131072, 8, 1)).unwrap();
            let error = reader
                .read(&access, &reduced, &[retained[2].0])
                .unwrap_err();
            assert!(error.contains("dependency chain depth"), "{error}");
        }
    }
    println!("sp1-refusals-custody-strict-v2 old sealed chunk depth1 read refusal/depth2 writer no-trial FULL and fixed 12-step whole-work vectors PASS");
}

#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required; definite custody gate"]
fn real_provider_definite_connection_refusal_abandons_without_retry() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let authority = closed.local_addr().unwrap().to_string();
    drop(closed);
    let path = disposable("refusal-definite-connect");
    let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
    let mut refused = provider.clone();
    refused.authority = authority;
    let mut writer = Writer::new(catalog.clone(), refused.clone(), capacities()).unwrap();
    let canonical = std::fs::read(evidence().join("old-producer-v1/whole0.canonical")).unwrap();
    writer
        .offer(
            LogicalUse::RegularFileGraph,
            FinalizedObject::new(ObjectRole::WholeFile, canonical).unwrap(),
        )
        .unwrap();
    let before = refused.statistics().unwrap().put_calls;
    let error = writer.finish_storage().unwrap_err();
    assert!(
        error.to_lowercase().contains("refused"),
        "controlled pre-send failure must be connection-refused: {error}"
    );
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let status: i64 = db
        .query_row(
            "SELECT status FROM saves WHERE save_id=?1",
            [writer.save],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        status, 4,
        "definite pre-send failure is abandoned; it cannot be Unknown custody"
    );
    assert!(catalog.publish(writer.save).is_err());
    let calls = refused.statistics().unwrap().put_calls;
    assert_eq!(calls - before, 1);
    assert!(writer.finish_storage().is_err());
    assert_eq!(refused.statistics().unwrap().put_calls, calls);
    println!("sp1-refusals-custody-strict-v2 definite pre-send connection refusal status4, one attempt, no publication/retry: {error}");
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required; definite registration gate"]
fn real_provider_definite_sql_registration_failure_retains_known_ack_body() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("refusal-definite-register");
    let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
    let fault = rusqlite::Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER sp1_reject_locator BEFORE INSERT ON locators BEGIN SELECT RAISE(ABORT,'sp1 external definite locator refusal'); END;").unwrap();
    drop(fault);
    let mut writer = Writer::new(catalog.clone(), provider.clone(), capacities()).unwrap();
    let canonical = std::fs::read(evidence().join("old-producer-v1/whole0.canonical")).unwrap();
    writer
        .offer(
            LogicalUse::RegularFileGraph,
            FinalizedObject::new(ObjectRole::WholeFile, canonical).unwrap(),
        )
        .unwrap();
    let before = provider.statistics().unwrap().put_calls;
    let error = writer.finish_storage().unwrap_err();
    assert!(
        error.contains("sp1 external definite locator refusal"),
        "must reach the external SQL trigger after actual upload ACK: {error}"
    );
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let status: i64 = db
        .query_row(
            "SELECT status FROM saves WHERE save_id=?1",
            [writer.save],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        status, 4,
        "definite registration rollback is abandoned while known provider custody stays retained"
    );
    let (digest, ready): (Vec<u8>, i64) = db
        .query_row(
            "SELECT digest,ready FROM bodies WHERE save_id=?1 AND domain=0",
            [writer.save],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ready, 1);
    let digest: [u8; 32] = digest.try_into().unwrap();
    assert_eq!(
        provider.get(&digest).unwrap(),
        std::fs::read(evidence().join("old-producer-v1/pack-1.row")).unwrap()
    );
    let locators: i64 = db
        .query_row("SELECT count(*) FROM locators", [], |row| row.get(0))
        .unwrap();
    assert_eq!(locators, 0);
    assert!(catalog.publish(writer.save).is_err());
    let calls = provider.statistics().unwrap().put_calls;
    assert_eq!(calls - before, 1);
    assert!(writer.finish_storage().is_err());
    assert_eq!(provider.statistics().unwrap().put_calls, calls);
    println!("sp1-refusals-custody-strict-v2 definite registration abort status4, original96097B known ACK body retained, no publication/retry/delete: {error}");
}
