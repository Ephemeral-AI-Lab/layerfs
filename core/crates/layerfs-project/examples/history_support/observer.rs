//! Optional benchmark observer snapshots; no product hook or altered operation.
use std::ffi::{c_char, c_void};
extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
pub fn snapshot() -> [u64; 20] {
    let mut out = [0; 20];
    // macOS RTLD_DEFAULT; the qualified runner pins the first-party dylib.
    let pointer = unsafe { dlsym((-2isize) as *mut c_void, c"cause_history_snapshot".as_ptr()) };
    if !pointer.is_null() {
        let call: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(pointer) };
        unsafe { call(out.as_mut_ptr()) };
    }
    out
}
pub fn report(state: usize, stage: &str, before: [u64; 20]) {
    let after = snapshot();
    let difference = std::array::from_fn::<_, 19, _>(|n| after[n].saturating_sub(before[n]));
    eprintln!("HISTORY_ENGINE_WORK {{\"state\":{state},\"stage\":\"{stage}\",\"available\":{},\"values\":{:?}}}",after[19]==1,difference);
}

/// Optional exact BLOB-call counters, separate from SQL/VFS and device bytes.
pub fn blob_snapshot() -> [u64; 12] {
    let mut out = [0; 12];
    let pointer = unsafe {
        dlsym(
            (-2isize) as *mut c_void,
            c"cause_history_blob_snapshot".as_ptr(),
        )
    };
    if !pointer.is_null() {
        let call: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(pointer) };
        unsafe { call(out.as_mut_ptr()) };
    }
    out
}
/// Diagnostic phase snapshot; contains no product data or cache warming.
pub fn phase_start() -> ([u64; 20], [u64; 12]) {
    (snapshot(), blob_snapshot())
}
/// Child-phase differences are logged separately from their enclosing state.
pub fn phase_report(stage: &str, before: ([u64; 20], [u64; 12])) {
    let sql = snapshot();
    let blob = blob_snapshot();
    let sql_values = std::array::from_fn::<_, 19, _>(|n| sql[n].saturating_sub(before.0[n]));
    let blob_values = std::array::from_fn::<_, 11, _>(|n| blob[n].saturating_sub(before.1[n]));
    eprintln!("VERIFY_PHASE_WORK {{\"stage\":\"{stage}\",\"available\":{},\"sql_values\":{:?},\"blob_values\":{:?}}}", sql[19] == 1 && blob[11] == 1, sql_values, blob_values);
}

/// Successful physical body extraction/BLOB bytes, including dependencies.
/// Conservative across all BLOB handles; unsupported observer fails closed.
pub fn acquired_snapshot() -> Option<u64> {
    let pointer = unsafe {
        dlsym(
            (-2isize) as *mut c_void,
            c"cause_history_acquired_snapshot".as_ptr(),
        )
    };
    if pointer.is_null() {
        return None;
    }
    let mut out = [0; 2];
    let call: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(pointer) };
    unsafe { call(out.as_mut_ptr()) };
    (out[1] == 1).then_some(out[0])
}
