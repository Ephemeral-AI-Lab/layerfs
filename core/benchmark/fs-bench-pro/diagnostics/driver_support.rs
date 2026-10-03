//! Shared diagnostic presentation. No product implementation is imported.
use std::{fmt::Write as _, time::Instant};
#[repr(C)]
struct Timespec {
    seconds: i64,
    nanos: i64,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, value: *mut Timespec) -> i32;
}
pub fn cpu_ns() -> u64 {
    let mut value = Timespec {
        seconds: 0,
        nanos: 0,
    };
    #[cfg(target_os = "macos")]
    let clock = 12;
    #[cfg(not(target_os = "macos"))]
    let clock = 2;
    assert_eq!(
        unsafe { clock_gettime(clock, &mut value) },
        0,
        "process CPU clock unavailable"
    );
    value.seconds as u64 * 1_000_000_000 + value.nanos as u64
}
pub fn hex(bytes: &[u8]) -> String {
    let mut value = String::new();
    for byte in bytes {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}
pub fn unhex<const N: usize>(text: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    if text.len() != N * 2 {
        return Err("hex width".into());
    }
    let mut out = [0; N];
    for (i, value) in out.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}
pub struct Phases(Vec<(&'static str, u128, u64)>);
impl Phases {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn measure<T, E>(
        &mut self,
        name: &'static str,
        f: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let cpu = cpu_ns();
        let start = Instant::now();
        let result = f();
        self.0
            .push((name, start.elapsed().as_nanos(), cpu_ns() - cpu));
        result
    }
    pub fn json(&self) -> String {
        let mut s = String::from("[");
        for (i, (name, wall, cpu)) in self.0.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            write!(
                s,
                "{{\"phase\":\"{name}\",\"wall_ns\":{wall},\"cpu_ns\":{cpu}}}"
            )
            .unwrap();
        }
        s.push(']');
        s
    }
}
