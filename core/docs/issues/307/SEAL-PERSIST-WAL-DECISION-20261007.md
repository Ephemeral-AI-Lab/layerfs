# Host seal: Apple persistent WAL

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owner approved 2026-10-07 during F1–F4: "Approve the narrowly audited seal wrapper".

The new host proof fails closed because system SQLite 3.51.0 leaves its empty
WAL and shared-memory sidecars after checkpoint and checked close. Receipt
[`04-system-sqlite-seal-diagnostic.stdout`](checks/pre-s8-store-foundation-20261007/04-system-sqlite-seal-diagnostic.stdout)
observes `SQLITE_FCNTL_PERSIST_WAL = 1` and reproduces the same behavior directly
against `/usr/lib/libsqlite3.dylib`. This is a functional mechanism diagnostic,
not a benchmark. [`03`](checks/pre-s8-store-foundation-20261007/03-host-serverless-store.stdout)
retains the failed product proof.

SQLite documents this setting in its [WAL lifecycle](https://www.sqlite.org/wal.html)
and [file-control contract](https://www.sqlite.org/c3ref/c_fcntl_begin_atomic_write.html#sqlitefcntlpersistwal).
The locked rusqlite 0.40.2 source provides no safe file-control wrapper. The
existing `Connection::handle()` and `rusqlite::ffi::sqlite3_file_control` require
an unsafe call. Persistence's crate attribute and the
[boundary guard](../../../tools/check_product_boundary.py) forbid it today.

## Approved exception

Add one audited `backend/sqlite/file_control.rs` module, used only after seal
has acquired sole in-process ownership and before its sole checkpoint/close:

```rust
// All calls run synchronously while the live Connection is exclusively owned.
// SQLite reads/writes this stack integer only during sqlite3_file_control.
let mut persist: std::ffi::c_int = 0;
let rc = unsafe {
    rusqlite::ffi::sqlite3_file_control(
        connection.handle(),
        c"main".as_ptr(),
        rusqlite::ffi::SQLITE_FCNTL_PERSIST_WAL,
        (&mut persist as *mut std::ffi::c_int).cast(),
    )
};
// Refuse a non-OK result; query back with persist = -1 and require zero.
// Do not retry, delete sidecars manually, change journal mode or switch SQLite.
```

Use `deny(unsafe_code)` at the Persistence crate root, allow it only on that
module and extend the existing audited-module guard mapping/self-tests for
this exact path. Keep every other Persistence file free of unsafe Rust.
No dependency, third-party patch, engine swap or shell helper is introduced.
The same path is implemented for both profiles, with Disposable-only execution.
Exact errors and sidecar verification remain required after checked close.

The owner explicitly approved this exact exception in the active implementation
chat. The wrapper is compiled on macOS only; Linux retains its already-proven
checkpoint/close path. The boundary guard and its self-test authorize only
`src/backend/sqlite/file_control.rs`. No third-party code changed. Final host
proof remains separately recorded; approval itself is not proof.
