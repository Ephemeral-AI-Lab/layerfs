//! External Linux functional consumer of a separately initialized real host.
//! Socket/process watchdogs are proof bounds, not product runtime limits.
use layerfs_bridge::{
    codec::{ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
    native,
};
use layerfs_daemon::{upstream::Upstream, Owner, OwnerConfig};
use layerfs_overlay::ProfileConfig;
use layerfs_sdk::client::Attachment;
use std::{
    fmt::Debug,
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    time::{Duration, Instant},
};

#[path = "upstream_consumer/assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "upstream_consumer/exercise.rs"]
mod exercise;

fn fail(error: impl Debug) -> ! {
    // Leave original owners in scope until process termination. A proof refusal
    // does not authorize guessed Close, source release, reconnect or replay.
    eprintln!("R4_REAL_HOST_CONSUMER_FAIL {error:?}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        fail("usage: upstream_consumer endpoint assignment_path");
    }
    let endpoint = args[1].to_str().unwrap_or_else(|| fail("endpoint UTF-8"));
    let assignment_path = PathBuf::from(&args[2]);
    // Independently exits even if a blocking product owner or its Drop hangs.
    // The owning Docker/test watchdog is tighter and retains the failed output.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(90));
        eprintln!("R4_REAL_HOST_CONSUMER_FAIL external functional watchdog 90s");
        std::process::exit(2);
    });
    if !cfg!(target_os = "linux") {
        fail("external consumer proof requires Linux");
    }
    let (expected, bootstrap) = assignment::read(&assignment_path).unwrap_or_else(|e| fail(e));
    let local = native::public_key(&[1; 32]).unwrap_or_else(|e| fail(e));
    if expected.local_peer != local {
        fail("assignment differs from fixed external client key");
    }
    let address = endpoint
        .to_socket_addrs()
        .unwrap_or_else(|e| fail(e))
        .next()
        .unwrap_or_else(|| fail("endpoint returned no first address"));
    let stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap_or_else(|e| fail(e));
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap_or_else(|e| fail(e));
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap_or_else(|e| fail(e));
    let connection =
        native::initiate(stream, &[1; 32], expected.host_peer).unwrap_or_else(|e| fail(e));
    let budget = ReceiveBudget::new(ReassemblyConfig {
        kind: MessageKind::Reply,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 34 << 20,
        bytes: 96 << 20,
        demand_reserve: 34 << 20,
        control_reserve: 64 << 10,
    })
    .unwrap_or_else(|e| fail(e));
    let attachment = Attachment::new(connection, budget).unwrap_or_else(|(error, connection)| {
        match &error {
            layerfs_sdk::client::AttachmentError::Native(e) => eprintln!("attachment native: {e}"),
            layerfs_sdk::client::AttachmentError::Frame(e) => eprintln!("attachment frame: {e}"),
        }
        // Retain the original connection until the deciding refusal is printed.
        let _original = connection;
        fail("original consumer attachment refused")
    });
    let calls = attachment.calls();
    let owned = PathBuf::from(format!("/tmp/layerfs-r4-consumer-{}", std::process::id()));
    std::fs::create_dir(&owned).unwrap_or_else(|e| fail(e));
    let owner = Owner::start(
        &owned.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap_or_else(|e| fail(e));
    let mut upstream = Upstream::attach(attachment, expected, bootstrap, owner.client(), 8192)
        .unwrap_or_else(|refusal| fail(&refusal));
    exercise::run(
        &upstream,
        &calls,
        &assignment_path.with_extension("metadata"),
    )
    .unwrap_or_else(|failure| fail(&failure));
    exercise::close(&upstream).unwrap_or_else(|e| fail(e));
    upstream.fence();
    let deadline = Instant::now() + Duration::from_secs(5);
    let fence = loop {
        if let Some(fence) = upstream.try_join().unwrap_or_else(|e| fail(e)) {
            break fence;
        }
        if Instant::now() >= deadline {
            fail("original consumer completion fence exceeded 5s");
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    if let Err(error) = &fence.close {
        fail(error);
    }
    if !fence.partial.is_empty() {
        fail("consumer fence retained unexpected partial replies");
    }
    let receive = fence.receive.as_ref().unwrap_or_else(|e| fail(e));
    if receive.live_messages != 0 || receive.credited_bytes != 0 {
        fail("original received reply owners still hold credit at consumer fence");
    }
    println!("R4_CONSUMER_FENCED receive={receive:?}");
    drop(fence);
    drop(calls);
    drop(upstream);
    owner.stop().unwrap_or_else(|e| fail(e));
    std::fs::remove_dir_all(&owned).unwrap_or_else(|e| fail(e));
    println!("R4_REAL_HOST_CONSUMER_PASS files=7 source_removed=true");
}
