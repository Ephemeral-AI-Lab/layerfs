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
