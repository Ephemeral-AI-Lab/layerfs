//! `sandbox.get` of a Sandbox on a machine whose daemon has no vacant
//! control slot (#649). This route runs the Workspace list in the machine's
//! worker, so the refusal its daemon gave comes back from the worker exactly
//! as it was made — `Capacity` on `daemon_control_slot`, with the vacant
//! slots and retained failures the admission notice carried, and no `code`.
//! The get reads that refusal the way it reads a forwarded daemon's: the
//! record with the Sandbox down, the refusal's own words as `down_reason`,
//! what it observed as the partial `down` counts, and no `daemon_capacity`
//! (a notice is never mapped into that model) — never a failed call.
//!
//! The worker here is a stand-in: it answers the secret, reads one request
//! and answers the refusal that its machine's daemon gave. What it does not
//! do is the worker's own dispatch, which is where the refusal is made; the
//! refusal is data from then on, and this is the caller's side of it.
#![cfg(unix)]
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, write_frame, Hello};
use layerfs_bridge::control::SdkValue;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Names the homes of one test process apart.
static HOMES: AtomicUsize = AtomicUsize::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lfs-scap-worker-{tag}-{}-{sequence}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A record from `fields`, in order.
fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

/// Machine `box` (a `docker` machine, so its Sandboxes run in its worker),
/// its Worker record at `worker`, Store `shop` and project `api`.
fn home_records(home: &Path, worker: SocketAddr) {
    write_record(
        &paths::machine_record(home, "box"),
        &map(&[("name", t("box")), ("platform", t("docker"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(home, "box"),
        &map(&[("address", t(&worker.to_string())), ("secret", t("c0ffee"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(home, "shop"),
        &map(&[
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("box")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, "shop", "api"),
        &map(&[("name", t("api"))]),
    )
    .unwrap();
}

/// Sandbox `s1` on `box`, recorded with its `id` and no forward: this home
/// reaches it through that machine's worker.
fn sandbox_record(home: &Path) {
    let fields = [
        ("name", t("s1")),
        ("project", t("shop/api")),
        ("machine", t("box")),
        ("id", t("s-1")),
    ];
    write_record(&paths::sandbox_record(home, "s1"), &map(&fields)).unwrap();
}

/// The refusal a daemon whose pool has no vacant control slot gives before
/// any handshake, as it comes back from the worker that ran the call.
fn full_pool() -> SdkValue {
    refused(
        "Capacity",
        "the Sandbox's daemon has no vacant control slot: 0 pool slot(s) and 1 \
         reserved slot(s) are vacant, with 3 retained failure record(s). The call was not \
         admitted; retrying is safe",
        SdkValue::map([
            ("resource", t("daemon_control_slot")),
            ("retained", SdkValue::Int(3)),
            ("pool_vacant", SdkValue::Int(0)),
            ("reserved_vacant", SdkValue::Int(1)),
            ("retryable", SdkValue::Bool(true)),
            ("admission", t("no-pool-slot")),
        ]),
    )
}

/// Refusal `code` with `detail`, in the frame a worker answers a request it
/// refused.
fn refused(code: &str, message: &str, detail: SdkValue) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([
            ("code", t(code)),
            ("message", t(message)),
            ("detail", detail),
        ]),
    )])
}

/// A worker that answers its secret, reads one request and refuses it with
/// `answer`, counting the requests it read.
fn refusing_worker(answer: SdkValue) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
            // the caller's secret, answered before it sends anything
            if read_frame(&mut stream).is_err() {
                continue;
            }
            if write_frame(&mut stream, &Hello::own().to_frame()).is_err() {
                continue;
            }
            // one request, answered once: nothing is resent, and the get
            // dials nothing for the capacity model
            let Ok(Some(_)) = read_frame(&mut stream) else {
                continue;
            };
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = write_frame(&mut stream, &answer);
            let _ = stream.flush();
        }
    });
    (endpoint, asked)
}

#[test]
fn a_full_pool_s_get_reads_the_worker_route_like_the_forwarded_one() {
    let (endpoint, asked) = refusing_worker(full_pool());
    let home = tempdir("full");
    home::open(&home).unwrap();
    home_records(&home, endpoint);
    sandbox_record(&home);
    // the record is answered, not refused: a full pool is why a get exists
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    assert_eq!(asked.load(Ordering::SeqCst), 1, "asked {asked:?} times");
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");
    // the Workspace list is what was refused, so there is no page to show
    assert_eq!(
        got.get("workspaces"),
        Some(&SdkValue::List(Vec::new())),
        "{got:?}"
    );
    let reason = got
        .get_text("down_reason")
        .unwrap_or_else(|| panic!("{got:?}"));
    assert!(reason.contains("no vacant control slot"), "{reason}");
    // and the counts are readable, in the same partial model the forwarded
    // route answers: the notice's five keys and nothing else
    let down = got.get("down").unwrap_or_else(|| panic!("{got:?}"));
    assert_eq!(down.get_text("code"), Some("Capacity"), "{down:?}");
    assert_eq!(
        down.get_text("resource"),
        Some("daemon_control_slot"),
        "{down:?}"
    );
    assert_eq!(down.get_int("pool_vacant"), Some(0), "{down:?}");
    assert_eq!(down.get_int("reserved_vacant"), Some(1), "{down:?}");
    assert_eq!(down.get_int("retained"), Some(3), "{down:?}");
    let SdkValue::Map(fields) = down else {
        panic!("{down:?}")
    };
    assert_eq!(fields.len(), 5, "{down:?}");
    // the notice is partial: it is never dressed up as the full model, and
    // the get dials nothing for one
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(asked.load(Ordering::SeqCst), 1, "asked {asked:?} times");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn another_capacity_or_a_busy_refusal_still_fails_the_get() {
    // only a full pool's own refusal reads as a down Sandbox: `Capacity` on
    // another resource and a refusal with another code are the refusals they
    // are, so a get never hides one behind `down_reason`
    for (code, detail) in [
        ("Capacity", SdkValue::map([("resource", t("store_lock"))])),
        ("Busy", SdkValue::Null),
    ] {
        let (endpoint, asked) = refusing_worker(refused(code, "the pool says so", detail));
        let home = tempdir("other");
        home::open(&home).unwrap();
        home_records(&home, endpoint);
        sandbox_record(&home);
        let error = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap_err();
        assert_eq!(asked.load(Ordering::SeqCst), 1, "asked {asked:?} times");
        assert_eq!(error.0, code, "{error:?}");
        assert_eq!(error.1, "the pool says so", "{error:?}");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn a_capacity_on_a_slot_that_observed_no_counts_still_fails_the_get() {
    // a `Capacity` an admitted connection returned is collapsed to `{phase}`
    // by `values::refusal` (#547 slice 6): it names no pool and carries no
    // counts, so it is not a full pool's notice and the get must fail as it
    // did before rather than answer an invented down
    let detail = SdkValue::map([("phase", t("receive"))]);
    let (endpoint, asked) = refusing_worker(refused("Capacity", "a slot said so", detail));
    let home = tempdir("phase");
    home::open(&home).unwrap();
    home_records(&home, endpoint);
    sandbox_record(&home);
    let error = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap_err();
    assert_eq!(asked.load(Ordering::SeqCst), 1, "asked {asked:?} times");
    assert_eq!(error.0, "Capacity", "{error:?}");
    assert_eq!(error.1, "a slot said so", "{error:?}");
    std::fs::remove_dir_all(home).unwrap();
}
