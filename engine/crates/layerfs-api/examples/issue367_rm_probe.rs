//! Issue #367 diagnostic: one-file `fs.rm` Store work, statement by statement.
//!
//! Build with `LIBSQLITE3_FLAGS=-DSQLITE_ENABLE_SQLLOG` (diagnostic only): the
//! SQLLOG hook installs a profile trace on every connection, and each statement
//! on the Store (a file under /v/w/, the probe volume; not the private
//! overlay) is attributed to the thread that ran it, by SQL text. Reading the VM counter does not reset it, so the product's
//! own `SqlWork` stays valid.
//!
//!   issue367_rm_probe <store> <binding hex> list <dir>
//!   issue367_rm_probe <store> <binding hex> rm <path>...
//!       one Branch forked from main; each path removed by its own fs.rm, in turn
//!   issue367_rm_probe <store> <binding hex> contend <same|other> <path> <path>...
//!       the first path removed while a second thread removes the rest one by
//!       one, on the same Branch or on another one
use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_store_service::{telemetry, StoreService};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::{c_char, c_int, c_uint, c_void, CStr},
    sync::{atomic::AtomicBool, atomic::Ordering, Arc},
    time::Instant,
};

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_config(option: c_int, ...) -> c_int;
    fn sqlite3_trace_v2(
        db: *mut c_void,
        mask: c_uint,
        callback: Option<
            unsafe extern "C" fn(c_uint, *mut c_void, *mut c_void, *mut c_void) -> c_int,
        >,
        context: *mut c_void,
    ) -> c_int;
    fn sqlite3_sql(stmt: *mut c_void) -> *const c_char;
    fn sqlite3_stmt_status(stmt: *mut c_void, status: c_int, reset: c_int) -> c_int;
    fn sqlite3_db_filename(db: *mut c_void, name: *const c_char) -> *const c_char;
}

#[derive(Default, Clone, Copy)]
struct Row {
    calls: u64,
    vm: u64,
    ns: u64,
}
thread_local! {
    static ROWS: RefCell<BTreeMap<String, Row>> = const { RefCell::new(BTreeMap::new()) };
    static LAST_VM: RefCell<BTreeMap<usize, i64>> = const { RefCell::new(BTreeMap::new()) };
}
static INSTALLED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn trace(
    _: c_uint,
    db: *mut c_void,
    stmt: *mut c_void,
    ns: *mut c_void,
) -> c_int {
    let file = unsafe { sqlite3_db_filename(db, c"main".as_ptr()) };
    if file.is_null()
        || !unsafe { CStr::from_ptr(file) }
            .to_bytes()
            .starts_with(b"/v/w/")
    {
        return 0;
    }
    let sql = unsafe { sqlite3_sql(stmt) };
    if sql.is_null() {
        return 0;
    }
    let text = unsafe { CStr::from_ptr(sql) }.to_string_lossy();
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // The product resets the counter after reading it; a profile event before
    // that reset sees this run's steps, one after it sees zero.
    let steps = unsafe { sqlite3_stmt_status(stmt, 4, 0) } as i64;
    let delta = LAST_VM.with_borrow_mut(|last| {
        let previous = last.insert(stmt as usize, steps).unwrap_or(0);
        if steps >= previous {
            steps - previous
        } else {
            steps
        }
    });
    let ns = if ns.is_null() {
        0
    } else {
        unsafe { *(ns as *const u64) }
    };
    ROWS.with_borrow_mut(|rows| {
        let row = rows.entry(text).or_default();
        row.calls += 1;
        row.vm += delta.max(0) as u64;
        row.ns += ns;
    });
    0
}
unsafe extern "C" fn opened(_: *mut c_void, db: *mut c_void, _: *const c_char, event: c_int) {
    if event == 0 {
        unsafe { sqlite3_trace_v2(db, 2, Some(trace), db) };
    }
}
/// Called by SQLite's initialization when built with SQLITE_ENABLE_SQLLOG.
#[no_mangle]
pub extern "C" fn sqlite3_init_sqllog() {
    let code = unsafe {
        sqlite3_config(
            21,
            opened as unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char, c_int),
            std::ptr::null_mut::<c_void>(),
        )
    };
    INSTALLED.store(code == 0, Ordering::Relaxed);
}

fn take_rows() -> BTreeMap<String, Row> {
    ROWS.with_borrow_mut(std::mem::take)
}

fn call(service: &StoreService, method: &str, args: SdkValue) -> Result<SdkValue, String> {
    layerfs_api::call(
        service,
        &SdkCall {
            method: method.into(),
            args,
        },
    )
    .map_err(|r| format!("{r:?}"))
}

fn args(fields: &[(&str, &str)]) -> SdkValue {
    let mut all = vec![("project".to_owned(), SdkValue::text("dsh"))];
    all.extend(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), SdkValue::text(*v))),
    );
    SdkValue::Map(all)
}

fn sum(rows: &BTreeMap<String, Row>) -> Row {
    rows.values().fold(Row::default(), |a, r| Row {
        calls: a.calls + r.calls,
        vm: a.vm + r.vm,
        ns: a.ns + r.ns,
    })
}

/// One traced fs.rm: its value, phase tree, own statements by text, and the
/// writer/readers session deltas (which include any concurrent call).
fn rm(service: &StoreService, branch: &str, path: &str, label: &str, detail: bool) {
    take_rows();
    let sessions = service.opened.sql_sessions();
    let before = sessions.totals();
    let recording = telemetry::Recording::start();
    let start = Instant::now();
    let result = call(
        service,
        "fs.rm",
        args(&[
            ("branch", branch),
            ("path", path),
            ("message", "issue367 probe"),
        ]),
    );
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let reports = recording.finish();
    let session = telemetry::sql(before, sessions.totals());
    let rows = take_rows();
    let own = sum(&rows);
    println!(
        "{{\"label\":\"{label}\",\"path\":\"{path}\",\"ms\":{ms:.3},\"ok\":{},\"own_statements\":{},\"own_vm\":{},\"own_sql_ms\":{:.3},\"sessions\":\"{}\"}}",
        result.is_ok(),
        own.calls,
        own.vm,
        own.ns as f64 / 1e6,
        format!("{:?}", session.map(|s| summary(&s))).replace('"', "'")
    );
    if let Err(e) = &result {
        println!("  error {e}");
    }
    if detail {
        for report in &reports {
            let mut out = Vec::new();
            let _ = report.write_json(&mut out);
            println!("  timing {}", String::from_utf8_lossy(&out).trim());
        }
        let mut sorted: Vec<_> = rows.into_iter().collect();
        sorted.sort_by(|a, b| b.1.vm.cmp(&a.1.vm));
        for (sql, row) in sorted {
            println!(
                "  sql calls {:4} vm {:7} us {:8.1} | {}",
                row.calls,
                row.vm,
                row.ns as f64 / 1e3,
                &sql[..sql.len().min(220)]
            );
        }
    }
}

fn summary(value: &SdkValue) -> Vec<(String, i64, i64, i64)> {
    let SdkValue::Map(groups) = value else {
        return vec![];
    };
    groups
        .iter()
        .map(|(name, group)| {
            let field = |n| match group.get(n) {
                Some(SdkValue::Int(v)) => *v,
                _ => -1,
            };
            (
                name.clone(),
                field("statements"),
                field("vm_steps"),
                field("write_commits"),
            )
        })
        .collect()
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (store, binding, mode, rest) = (&argv[0], &argv[1], argv[2].as_str(), &argv[3..]);
    let binding: Vec<u8> = (0..binding.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&binding[i..i + 2], 16).unwrap())
        .collect();
    let dir = std::env::temp_dir().join(format!("issue367-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key = layerfs_bridge::duplex::generate_keypair().unwrap().public;
    let service = Arc::new(
        StoreService::open_direct(
            std::path::Path::new(store),
            &binding,
            key,
            &dir.join("overlay.sqlite"),
        )
        .unwrap_or_else(|e| panic!("open: {e:?}")),
    );
    println!("{{\"sqllog\":{}}}", INSTALLED.load(Ordering::Relaxed));
    let fork = |name: &str| {
        call(
            &service,
            "branch.fork",
            args(&[("name", name), ("from", "main")]),
        )
        .expect("fork");
    };
    let pid = std::process::id();
    match mode {
        "list" => {
            // Every entry of each named directory on main, paged: the count
            // and the first names.
            for dir in rest {
                let (mut count, mut first, mut pos) = (0, Vec::new(), 0i64);
                loop {
                    let mut a = args(&[("branch", "main"), ("path", dir)]);
                    if let SdkValue::Map(fields) = &mut a {
                        fields.push(("pageSize".into(), SdkValue::Int(1000)));
                        fields.push(("pagePos".into(), SdkValue::Int(pos)));
                    }
                    let value = call(&service, "fs.list", a).expect("list");
                    if let Some(SdkValue::List(items)) = value.get("items") {
                        count += items.len();
                        for item in items.iter().take(12usize.saturating_sub(first.len())) {
                            first.push(item.get_text("name").unwrap_or("?").to_owned());
                        }
                    }
                    match value.get("nextPagePos") {
                        Some(SdkValue::Int(next)) => pos = *next,
                        _ => break,
                    }
                }
                println!(
                    "{{\"dir\":\"{dir}\",\"entries\":{count},\"first\":\"{}\"}}",
                    first.join(",")
                );
            }
        }
        "rm" => {
            let branch = format!("p367-{pid}");
            fork(&branch);
            for (i, path) in rest.iter().enumerate() {
                rm(&service, &branch, path, &format!("solo{i}"), true);
            }
        }
        "contend" => {
            let same = rest[0] == "same";
            let (a, b) = (format!("p367a-{pid}"), format!("p367b-{pid}"));
            fork(&a);
            fork(&b);
            let other = if same { a.clone() } else { b.clone() };
            let target = rest[1].clone();
            let others: Vec<String> = rest[2..].to_vec();
            let background = service.clone();
            let thread = std::thread::spawn(move || {
                for (i, path) in others.iter().enumerate() {
                    rm(&background, &other, path, &format!("bg{i}"), false);
                }
            });
            std::thread::sleep(std::time::Duration::from_millis(15));
            rm(&service, &a, &target, "contended", true);
            thread.join().expect("background thread");
        }
        other => panic!("unknown mode {other}"),
    }
    match Arc::try_unwrap(service) {
        Ok(service) => service.close().unwrap(),
        Err(_) => panic!("service still shared"),
    }
}
