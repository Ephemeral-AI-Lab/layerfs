//! The daemon's private config: its Store kind, route and redacted Debug.
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_bridge::{DaemonStore, KernelRoute, StoreAuthority};

fn manifest() -> StoreManifest {
    StoreManifest {
        provider: ProviderKind::Sqlite,
        locator: "/layerfs-store/global/store.sqlite".into(),
        profile: StoreProfile::Disposable,
        binding: vec![7; 32],
        cursor_key: [9; 32],
        stack: [0; 17],
        branch: [0; 17],
        root: [0; 32],
        bytes: 1,
        host_sqlite: "3.50.4".into(),
        daemon_sqlite: Some("3.50.4".into()),
    }
}

fn postgres() -> DaemonStore {
    DaemonStore::Postgres {
        url: "postgresql://lfs:secret-pw@host.docker.internal:5432/layerfs".into(),
        schema: "lfc_team".into(),
        synchronous_commit: "local".into(),
        authority: StoreAuthority {
            binding: vec![7; 32],
            cursor_key: [9; 32],
        },
    }
}

fn setup(store: DaemonStore, route: KernelRoute) -> DaemonSetup {
    DaemonSetup {
        listen: "0.0.0.0:30421".into(),
        private_key: [1; 32],
        control_peer: [2; 32],
        route,
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: 8,
            handshake_ms: 2000,
            idle_ttl_ms: 60_000,
            cache_bytes: 1 << 20,
            namespaces: 16,
            pager_kib: 1024,
            serial_low_water: 256,
        },
        store,
    }
}

#[test]
fn each_store_and_route_round_trips() {
    for route in [KernelRoute::Fuse, KernelRoute::Nfs] {
        for store in [
            DaemonStore::Sqlite {
                locator: "/layerfs-store/global/store.sqlite".into(),
                install: None,
            },
            DaemonStore::Sqlite {
                locator: "/layerfs-store/global/store.sqlite".into(),
                install: Some(manifest()),
            },
            postgres(),
        ] {
            let value = setup(store, route);
            assert_eq!(
                DaemonSetup::decode(&value.encode().unwrap()).unwrap(),
                value
            );
        }
    }
}

#[test]
fn a_postgresql_setup_never_prints_its_url_or_cursor_key() {
    let value = setup(postgres(), KernelRoute::Nfs);
    let printed = format!("{value:?}");
    assert!(value.opens_existing());
    assert!(!printed.contains("secret-pw"), "{printed}");
    assert!(printed.contains("lfc_team"));
}

#[test]
fn an_install_manifest_must_match_the_locator_and_be_disposable() {
    let mut m = manifest();
    m.locator = "/other/store.sqlite".into();
    for store in [
        DaemonStore::Sqlite {
            locator: "/layerfs-store/global/store.sqlite".into(),
            install: Some(m),
        },
        {
            let mut m = manifest();
            m.profile = StoreProfile::Durable;
            DaemonStore::Sqlite {
                locator: "/layerfs-store/global/store.sqlite".into(),
                install: Some(m),
            }
        },
    ] {
        assert!(setup(store, KernelRoute::Fuse).encode().is_err());
    }
}

#[test]
fn unknown_route_or_store_bytes_and_the_old_magic_are_refused() {
    let value = setup(
        DaemonStore::Sqlite {
            locator: "/x/store.sqlite".into(),
            install: None,
        },
        KernelRoute::Fuse,
    );
    let bytes = value.encode().unwrap();
    // the route byte sits after the fixed fields: flip it to an unknown value
    let store_field = bytes
        .windows(12)
        .position(|w| w == b"/x/store.sql")
        .unwrap();
    let mut bad_route = bytes.clone();
    bad_route[store_field - 1] = 9;
    assert!(DaemonSetup::decode(&bad_route).is_err());
    let mut bad_kind = bytes.clone();
    bad_kind[store_field - 1 + 1] = 9;
    assert!(DaemonSetup::decode(&bad_kind).is_err());
    let mut old = bytes;
    old[4] = 6;
    assert!(DaemonSetup::decode(&old).is_err());
}
