//! TLS identity, statement timeout and lost acknowledgement without retry.
mod support;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_metadata::{PgMetadata, TlsProfile};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    policy::StoragePolicy,
    port::*,
};
use std::{io, net::TcpListener, path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn registration(pack_id: i64) -> Registration {
    let body = vec![7; 64];
    let info = PackInfo {
        pack_id,
        domain: PackDomain::Metadata,
        key: ObjectKey::for_bytes(&body),
        length: body.len(),
    };
    Registration {
        packs: vec![RegisteredPack {
            info,
            body: Some(body),
        }],
        objects: vec![ObjectLocation {
            object_id: ObjectId::for_bytes(b"connection-record"),
            role: ObjectRole::DirectoryLeaf,
            canonical_length: 100,
            pack_id,
            group_number: 0,
            record_number: 0,
        }],
        ..Default::default()
    }
}
#[test]
fn cancelled_registration_is_uncertain_and_does_not_reopen_or_resend() {
    let config = support::config("timeout");
    let owner = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let allocation = owner
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    let mut control = support::control(&config);
    control.batch_execute(&format!("CREATE FUNCTION \"{}\".slow_pack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.25); RETURN NEW; END $$; CREATE TRIGGER slow_pack BEFORE INSERT ON \"{}\".pack FOR EACH ROW EXECUTE FUNCTION \"{}\".slow_pack();",config.schema,config.schema,config.schema)).unwrap();
    let mut short = config.clone();
    short.statement_timeout = Duration::from_millis(25);
    let client = PgMetadata::open(short).unwrap();
    let before = client.diagnostics().unwrap();
    assert_eq!(
        client.register(&registration(allocation.first_pack_id)),
        Err(MetadataError::Uncertain)
    );
    let after = client.diagnostics().unwrap();
    assert_eq!(after.operations - before.operations, 1);
    assert_eq!(after.sync_messages - before.sync_messages, 1);
    assert_eq!(client.policy(), Err(MetadataError::Uncertain));
    assert_eq!(client.diagnostics().unwrap(), after);
    let count: i64 = control
        .query_one(
            &format!("SELECT count(*) FROM \"{}\".pack", config.schema),
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    println!("DIAGNOSTIC pg-timeout {:?}", after);
}
fn dropped_register_proxy(
    config: &layerfs_metadata::PgConfig,
) -> (layerfs_metadata::PgConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut proxied = config.clone();
    proxied.port = listener.local_addr().unwrap().port();
    proxied.host = "127.0.0.1".to_owned();
    let host = config.host.clone();
    let port = config.port;
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (client, _) = listener.accept().await.unwrap();
            let server = tokio::net::TcpStream::connect((host.as_str(), port))
                .await
                .unwrap();
            let (mut client_read, mut client_write) = client.into_split();
            let (mut server_read, mut server_write) = server.into_split();
            let sending = tokio::spawn(async move {
                let _ = tokio::io::copy(&mut client_read, &mut server_write).await;
            });
            let mut ready = 0;
            loop {
                let mut header = [0; 5];
                server_read.read_exact(&mut header).await.unwrap();
                let length = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
                assert!((4..=64 * 1024 * 1024).contains(&length));
                let mut body = vec![0; length - 4];
                server_read.read_exact(&mut body).await.unwrap();
                if ready < 2 {
                    client_write.write_all(&header).await.unwrap();
                    client_write.write_all(&body).await.unwrap();
                }
                if header[0] == b'Z' {
                    ready += 1;
                    if ready == 3 {
                        break;
                    }
                }
            }
            sending.abort();
        });
    });
    (proxied, thread)
}
#[test]
fn dropped_committed_reply_is_uncertain_and_the_same_operation_is_not_replayed() {
    let config = support::config("lost");
    let owner = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let allocation = owner
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    let (proxied, proxy) = dropped_register_proxy(&config);
    let client = PgMetadata::open(proxied).unwrap();
    let batch = registration(allocation.first_pack_id);
    assert_eq!(client.register(&batch), Err(MetadataError::Uncertain));
    proxy.join().unwrap();
    let before = client.diagnostics().unwrap();
    assert_eq!(client.register(&batch), Err(MetadataError::Uncertain));
    assert_eq!(client.diagnostics().unwrap(), before);
    let mut found = Vec::new();
    owner
        .locate(&[batch.objects[0].object_id], &mut found)
        .unwrap();
    assert_eq!(found.len(), 1);
    println!("DIAGNOSTIC pg-lost-reply {:?}", before);
}
fn certificates() -> (native_tls::Identity, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target/phase7-agent/tls-fixtures")
        .join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&dir).unwrap();
    let ca = dir.join("ca.pem");
    let key = dir.join("key.pem");
    let identity = dir.join("identity.p12");
    let commands = [
        vec![
            "req".to_owned(),
            "-x509".to_owned(),
            "-newkey".to_owned(),
            "rsa:2048".to_owned(),
            "-nodes".to_owned(),
            "-days".to_owned(),
            "1".to_owned(),
            "-keyout".to_owned(),
            key.to_string_lossy().into_owned(),
            "-out".to_owned(),
            ca.to_string_lossy().into_owned(),
            "-subj".to_owned(),
            "/CN=localhost".to_owned(),
            "-addext".to_owned(),
            "subjectAltName=DNS:localhost,IP:127.0.0.1".to_owned(),
            "-addext".to_owned(),
            "extendedKeyUsage=serverAuth".to_owned(),
            "-addext".to_owned(),
            "keyUsage=critical,digitalSignature,keyEncipherment,keyCertSign".to_owned(),
        ],
        vec![
            "pkcs12".to_owned(),
            "-export".to_owned(),
            "-inkey".to_owned(),
            key.to_string_lossy().into_owned(),
            "-in".to_owned(),
            ca.to_string_lossy().into_owned(),
            "-out".to_owned(),
            identity.to_string_lossy().into_owned(),
            "-keypbe".to_owned(),
            "PBE-SHA1-3DES".to_owned(),
            "-certpbe".to_owned(),
            "PBE-SHA1-3DES".to_owned(),
            "-macalg".to_owned(),
            "SHA1".to_owned(),
            "-passout".to_owned(),
            "pass:fixture".to_owned(),
        ],
    ];
    for args in commands {
        let output = std::process::Command::new("openssl")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "certificate setup failed");
    }
    (
        native_tls::Identity::from_pkcs12(&std::fs::read(identity).unwrap(), "fixture").unwrap(),
        ca,
    )
}
fn tls_proxy(
    config: &layerfs_metadata::PgConfig,
    identity: native_tls::Identity,
) -> (layerfs_metadata::PgConfig, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut proxied = config.clone();
    proxied.host = "127.0.0.1".to_owned();
    proxied.port = listener.local_addr().unwrap().port();
    let host = config.host.clone();
    let port = config.port;
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut ssl = [0; 8];
            socket.read_exact(&mut ssl).await.unwrap();
            assert_eq!(ssl, [0, 0, 0, 8, 4, 210, 22, 47]);
            socket.write_all(b"S").await.unwrap();
            let acceptor = tokio_native_tls::TlsAcceptor::from(
                native_tls::TlsAcceptor::new(identity).unwrap(),
            );
            let Ok(mut tls) = acceptor.accept(socket).await else {
                return;
            };
            let mut server = tokio::net::TcpStream::connect((host.as_str(), port))
                .await
                .unwrap();
            let _: io::Result<_> = tokio::io::copy_bidirectional(&mut tls, &mut server).await;
        });
    });
    (proxied, thread)
}
#[test]
fn verified_tls_checks_the_chain_and_hostname_and_never_downgrades() {
    let config = support::config("tls");
    let owner = PgMetadata::create(config.clone(), StoragePolicy::frozen_default()).unwrap();
    let (identity, ca) = certificates();
    let (mut trusted, proxy) = tls_proxy(&config, identity.clone());
    trusted.tls = TlsProfile::Verified {
        ca_certificates: Some(ca.clone()),
        server_name: "localhost".to_owned(),
    };
    let client = PgMetadata::open(trusted).unwrap();
    let before = client.diagnostics().unwrap();
    assert_eq!(client.policy().unwrap(), StoragePolicy::frozen_default());
    let after = client.diagnostics().unwrap();
    assert_eq!(after.sync_messages - before.sync_messages, 1);
    assert_eq!(after.ready_for_query - before.ready_for_query, 1);
    drop(client);
    proxy.join().unwrap();
    let (mut wrong_name, proxy) = tls_proxy(&config, identity.clone());
    wrong_name.tls = TlsProfile::Verified {
        ca_certificates: Some(ca),
        server_name: "wrong.invalid".to_owned(),
    };
    assert!(PgMetadata::open(wrong_name).is_err());
    proxy.join().unwrap();
    let (mut untrusted, proxy) = tls_proxy(&config, identity);
    untrusted.tls = TlsProfile::Verified {
        ca_certificates: None,
        server_name: "localhost".to_owned(),
    };
    assert!(PgMetadata::open(untrusted).is_err());
    proxy.join().unwrap();
    let mut required = config;
    required.tls = TlsProfile::Verified {
        ca_certificates: None,
        server_name: "localhost".to_owned(),
    };
    assert!(PgMetadata::open(required).is_err());
    assert_eq!(owner.policy().unwrap(), StoragePolicy::frozen_default());
    println!("DIAGNOSTIC pg-tls {:?}", after);
}
