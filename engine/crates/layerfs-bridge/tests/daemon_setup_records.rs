//! The daemon's private config with a PostgreSQL Store (#506).
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonPostgres, DaemonSetup};

fn setup() -> DaemonSetup {
    DaemonSetup {
        listen: "0.0.0.0:30421".into(),
        private_key: [1; 32],
        control_peer: [2; 32],
        store: "/layerfs-store/global/store.sqlite".into(),
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: 8,
            handshake_ms: 2000,
            cache_bytes: 1 << 20,
            namespaces: 16,
            pager_kib: 1024,
            serial_low_water: 256,
            idle_exit_s: 120,
        },
        existing_store: None,
        postgresql: Some(DaemonPostgres {
            url: "postgresql://lfs:secret-pw@host.docker.internal:5432/layerfs".into(),
            schema: "lfc_team".into(),
            synchronous_commit: "local".into(),
            binding: vec![7; 32],
            cursor_key: [9; 32],
        }),
    }
}

#[test]
fn a_postgresql_store_round_trips_and_its_url_never_prints() {
    let value = setup();
    let decoded = DaemonSetup::decode(&value.encode().unwrap()).unwrap();
    assert!(decoded == value);
    assert!(decoded.opens_existing());
    let printed = format!("{decoded:?}");
    assert!(!printed.contains("secret-pw"), "{printed}");
    assert!(printed.contains("lfc_team"));
}

#[test]
fn a_config_names_one_store_and_a_valid_schema_and_setting() {
    let mut both = setup();
    let mut sqlite = setup();
    sqlite.postgresql = None;
    assert!(!sqlite.opens_existing());
    both.existing_store = Some(layerfs_bridge::provision::StoreManifest {
        provider: layerfs_bridge::provision::ProviderKind::Sqlite,
        locator: both.store.clone(),
        profile: layerfs_bridge::provision::StoreProfile::Disposable,
        binding: vec![7; 32],
        cursor_key: [9; 32],
        stack: [0; 17],
        branch: [0; 17],
        root: [0; 32],
        bytes: 1,
        host_sqlite: "3.50.4".into(),
        daemon_sqlite: Some("3.50.4".into()),
    });
    assert!(both.encode().is_err());
    for (schema, setting) in [("1team", "off"), ("Team", "off"), ("team", "full")] {
        let mut bad = setup();
        let p = bad.postgresql.as_mut().unwrap();
        (p.schema, p.synchronous_commit) = (schema.into(), setting.into());
        assert!(bad.encode().is_err(), "{schema} {setting}");
    }
}
