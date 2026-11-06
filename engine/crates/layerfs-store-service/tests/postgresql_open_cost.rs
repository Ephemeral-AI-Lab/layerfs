//! #63/#64/#65: connections and round trips of PostgreSQL session opens,
//! counted on the wire by a loopback proxy in front of the server: each
//! server-to-client flight after a client flight is one round trip the
//! client waited for (TCP handshakes are not counted). Needs
//! `LAYERFS_TEST_POSTGRES_URL` with `sslmode=disable`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
use layerfs_history::records::StackInitialization;
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, Page,
};
use layerfs_persistence::{PostgresCheck, PostgresHandles, PostgresSelection};
use layerfs_storage::{ReservationBlocks, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

const BINDING: &[u8] = b"store-service-open-cost";
const CURSOR: [u8; 32] = [43; 32];

#[derive(Default)]
struct Wire {
    connections: AtomicU64,
    round_trips: AtomicU64,
    /// The server side of every proxied connection, for [`Wire::cut`].
    servers: std::sync::Mutex<Vec<TcpStream>>,
}
impl Wire {
    /// Ends every connection made so far, as a server restart would.
    fn cut(&self) {
        for server in self.servers.lock().unwrap().drain(..) {
            let _ = server.shutdown(std::net::Shutdown::Both);
        }
    }
    fn take(&self) -> (u64, u64) {
        (
            self.connections.swap(0, Ordering::SeqCst),
            self.round_trips.swap(0, Ordering::SeqCst),
        )
    }
}

/// A proxy to `upstream` on a loopback port; its port.
fn proxy(upstream: String, wire: Arc<Wire>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { return };
            let Ok(server) = TcpStream::connect(&upstream) else {
                return;
            };
            wire.connections.fetch_add(1, Ordering::SeqCst);
            wire.servers
                .lock()
                .unwrap()
                .push(server.try_clone().unwrap());
            // 1: the client spoke last, 2: the server did
            let last = Arc::new(AtomicU8::new(0));
            for (mut from, mut to, side) in [
                (
                    client.try_clone().unwrap(),
                    server.try_clone().unwrap(),
                    1u8,
                ),
                (server, client, 2u8),
            ] {
                let (wire, last) = (wire.clone(), last.clone());
                std::thread::spawn(move || {
                    let mut buffer = vec![0; 64 * 1024];
                    while let Ok(n) = from.read(&mut buffer) {
                        if n == 0 {
                            break;
                        }
                        if last.swap(side, Ordering::SeqCst) == 1 && side == 2 {
                            wire.round_trips.fetch_add(1, Ordering::SeqCst);
                        }
                        if to.write_all(&buffer[..n]).is_err() {
                            break;
                        }
                    }
                    let _ = to.shutdown(std::net::Shutdown::Both);
                });
            }
        }
    });
    port
}

/// `url` with its `host:port` replaced by the proxy's.
fn through(url: &str, port: u16) -> (String, String) {
    let at = url.find('@').unwrap() + 1;
    let end = at + url[at..].find('/').unwrap();
    let upstream = url[at..end].to_owned();
    (
        format!("{}127.0.0.1:{port}{}", &url[..at], &url[end..]),
        upstream,
    )
}

/// Waits until the proxy forwarded the last reply (bounded).
fn settle() {
    std::thread::sleep(std::time::Duration::from_millis(30));
}

#[test]
fn postgresql_open_costs_on_the_wire() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let wire = Arc::new(Wire::default());
    let (proxied, upstream) = through(&url, 0);
    let port = proxy(upstream, wire.clone());
    let proxied = proxied.replace(":0/", &format!(":{port}/"));
    let schema = format!("lfs63_cost_{}", std::process::id());
    let selection = PostgresSelection::new(&proxied, &schema);
    let stack = LayerStackId::from_authority([7; 16]);
    let id = |tag: u8| layerfs_content::ObjectId::for_bytes(&[tag]);
    let result = std::panic::catch_unwind(|| {
        let created = PostgresHandles::create(
            &selection,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: CURSOR,
                incarnation: 2,
            },
        )
        .unwrap();
        created
            .history
            .initialize_layerstack(&StackInitialization {
                logical: 0,
                stack,
                name: HistoryName::new("demo").unwrap(),
                scope: id(1),
                profile: id(2),
                genesis_root: id(3),
            })
            .unwrap();
        // a Branch to mount (#64)
        let demo = created
            .history
            .layer_stack_named(&HistoryName::new("demo").unwrap())
            .unwrap()
            .unwrap();
        created
            .history
            .fork(&ForkRequest {
                stack: demo.id,
                branch: BranchId::from_authority([8; 16]),
                name: HistoryName::new("work").unwrap(),
                source: ForkSource::Layer(demo.head_layer),
            })
            .unwrap();
        drop(created);
        settle();
        wire.take();
        let mut rows = Vec::new();
        let mut row = |name: &str, wire: &Wire| {
            settle();
            let (c, r) = wire.take();
            eprintln!("COST {name}: connections {c}, round trips {r}");
            rows.push((name.to_owned(), c, r));
        };

        // one read-only session open, as a Workspace reader opened before
        let reader = PostgresHandles::open_read_only(&selection, BINDING, CURSOR).unwrap();
        row("session open (read-only)", &wire);
        drop(reader);
        let writer = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
        row("session open (writable)", &wire);
        drop(writer);
        let light = selection.clone().with_check(PostgresCheck::Light);
        let reader = PostgresHandles::open_read_only(&light, BINDING, CURSOR).unwrap();
        row("session open (read-only, light check)", &wire);
        drop(reader);

        // #65: the host side of a PostgreSQL sandbox.create now: one
        // light-checked session reads the binding and finds the project
        let (binding, attached) = PostgresHandles::attach_read_only(&light, CURSOR).unwrap();
        assert_eq!(binding, BINDING);
        let project = attached
            .history
            .layer_stack_named(&HistoryName::new("demo").unwrap())
            .unwrap();
        assert!(project.is_some());
        row("host create (attach + project lookup)", &wire);
        drop(attached);

        // #64: the caller's side of a mount (and a status) of a forwarded
        // Sandbox, as a new CLI process runs it now: one light-checked
        // session finds the project and the Branch (id and head) by name
        let (_, attached) = PostgresHandles::attach_read_only(&light, CURSOR).unwrap();
        let project = attached
            .history
            .layer_stack_named(&HistoryName::new("demo").unwrap())
            .unwrap()
            .unwrap();
        let branch = attached
            .history
            .branch_named(project.id, &HistoryName::new("work").unwrap())
            .unwrap();
        assert!(branch.is_some());
        row("host mount (held session: project + Branch)", &wire);
        drop(attached);

        // the same mount as it ran before #64: a Store service open (its
        // fully checked writer), then `project.list_branches` by name on its
        // control reader, opened at that first read
        let opened = open_store_observed(
            layerfs_persistence::StoreSelection::Postgres(selection.clone()),
            BINDING,
            CURSOR,
            4,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        // two reads, as the Store service's lookups make them
        let stack = opened
            .store
            .published(|h| h.layer_stack_named(&HistoryName::new("demo").unwrap()))
            .unwrap()
            .unwrap();
        let found = opened
            .store
            .published(|h| h.branch_named(stack.id, &HistoryName::new("work").unwrap()))
            .unwrap();
        assert!(found.is_some());
        row("host mount before #64 (Store open + Branch)", &wire);
        drop(opened);
        settle();
        wire.take();

        // the host side of a PostgreSQL sandbox.create, as it ran before
        // #65: the reach check's binding read, a Store open, the project list
        let binding = PostgresHandles::binding(&selection).unwrap();
        assert_eq!(binding, BINDING);
        let opened = open_store_observed(
            layerfs_persistence::StoreSelection::Postgres(selection.clone()),
            BINDING,
            CURSOR,
            4,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        let listed = opened
            .store
            .published(|h| {
                h.layer_stacks(&Page {
                    cursor: None,
                    limit: 64,
                })
            })
            .unwrap();
        assert_eq!(listed.records.len(), 1);
        row("host create (reach + Store open + list)", &wire);
        drop(opened);
        settle();
        wire.take();

        // a daemon's Store open (light, as a Sandbox daemon opens), then mounts
        let opened = open_store_observed(
            layerfs_persistence::StoreSelection::Postgres(light.clone()),
            BINDING,
            CURSOR,
            4,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        row("daemon Store open", &wire);
        let ws = |n: u8| layerfs_history::WorkspaceId::from_authority([n; 32]).unwrap();
        opened.store.mount_reader(ws(1)).unwrap();
        row("mount reader open (first)", &wire);
        opened.store.close_reader(ws(1)).unwrap();
        row("unmount reader close (first)", &wire);
        opened.store.mount_reader(ws(2)).unwrap();
        row("mount reader open (second)", &wire);
        opened.store.close_reader(ws(2)).unwrap();
        settle();
        wire.take();
        let found = opened
            .store
            .published(|h| h.layer_stack_named(&HistoryName::new("demo").unwrap()))
            .unwrap();
        assert!(found.is_some());
        row("control lookup (project by name)", &wire);
        // #64: the second mount took the first one's idle reader
        let reuse = opened.store.reuse_work();
        assert_eq!((reuse.readers_opened, reuse.readers_reused), (1, 1));
        // an idle reader whose connection ended (the proxy cuts every
        // connection) is dropped at the next mount, which opens a new one
        wire.cut();
        settle();
        wire.take();
        opened.store.mount_reader(ws(3)).unwrap();
        row("mount after the idle reader's connection ended", &wire);
        let reuse = opened.store.reuse_work();
        assert_eq!((reuse.readers_opened, reuse.readers_reused), (2, 1));
        drop(opened);
        rows
    });
    if let Ok(mut client) = postgres::Client::connect(&url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
