//! Optional diagnostic-only public VFS counter bridge. No product hook.
#[cfg(target_os = "macos")]
mod host {
    use std::{
        ffi::{c_char, c_void, CString},
        sync::{
            atomic::{AtomicU64, Ordering},
            OnceLock,
        },
    };
    type Snapshot = unsafe extern "C" fn(*mut u64, usize) -> i32;
    type Unit = unsafe extern "C" fn(*const c_char, u64) -> i32;
    struct Api {
        snapshot: Snapshot,
        unit: Unit,
    }
    static API: OnceLock<Api> = OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(1);
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    unsafe fn symbol(name: &std::ffi::CStr) -> Result<*mut c_void, Box<dyn std::error::Error>> {
        let p = unsafe { dlsym((-2isize) as *mut c_void, name.as_ptr()) };
        if p.is_null() {
            Err(format!("missing injected observer symbol {name:?}").into())
        } else {
            Ok(p)
        }
    }
    pub fn initialize(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
        if !enabled {
            return Ok(());
        }
        let initialize: unsafe extern "C" fn() -> i32 =
            unsafe { std::mem::transmute(symbol(c"cause_vfs_initialize")?) };
        let snapshot: Snapshot =
            unsafe { std::mem::transmute(symbol(c"cause_init_vfs_snapshot")?) };
        let unit: Unit = unsafe { std::mem::transmute(symbol(c"cause_init_vfs_unit")?) };
        if unsafe { initialize() } != 0 {
            return Err("delegated VFS initialization refused".into());
        }
        API.set(Api { snapshot, unit })
            .map_err(|_| "observer initialized twice")?;
        Ok(())
    }
    fn snapshot(api: &Api) -> [u64; 32] {
        let mut values = [0; 32];
        assert_eq!(
            unsafe { (api.snapshot)(values.as_mut_ptr(), values.len()) },
            0
        );
        values
    }
    pub struct Scope {
        held: Option<(&'static str, u64, [u64; 32])>,
    }
    pub fn span(name: &'static str) -> Scope {
        let held = API.get().map(|api| {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let label = CString::new(name).unwrap();
            assert_eq!(unsafe { (api.unit)(label.as_ptr(), id) }, 0);
            (name, id, snapshot(api))
        });
        Scope { held }
    }
    impl Scope {
        pub fn finish(self) {
            if let Some((name, id, before)) = self.held {
                let api = API.get().unwrap();
                let after = snapshot(api);
                let mut delta = [[0; 8]; 4];
                for k in 0..4 {
                    for j in 0..8 {
                        delta[k][j] = after[k * 8 + j]
                            .checked_sub(before[k * 8 + j])
                            .expect("monotonic VFS counters");
                    }
                }
                assert_eq!(unsafe { (api.unit)(c"".as_ptr(), 0) }, 0);
                println!("VFS_UNIT {{\"unit\":\"{name}\",\"unit_id\":{id},\"classes\":{delta:?}}}");
            }
        }
    }
}
#[cfg(target_os = "macos")]
pub use host::{initialize, span};
#[cfg(not(target_os = "macos"))]
pub fn initialize(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    if enabled {
        Err("delegated global Store VFS observer requires macOS".into())
    } else {
        Ok(())
    }
}
#[cfg(not(target_os = "macos"))]
pub struct Scope;
#[cfg(not(target_os = "macos"))]
pub fn span(_: &'static str) -> Scope {
    Scope
}
#[cfg(not(target_os = "macos"))]
impl Scope {
    pub fn finish(self) {}
}
